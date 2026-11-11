use super::*;

struct Container(String);
impl Drop for Container {
    fn drop(&mut self) {
        let _ = execute(Command::new("docker").args(["rm", "-f", &self.0]), None, 30);
    }
}

fn output(command: &mut Command) -> String {
    let output = command.output().expect("Cannot run fixture inspection");
    assert!(output.status.success(), "Fixture inspection failed");
    String::from_utf8(output.stdout).expect("Non-UTF8 fixture inspection")
}

fn cli(config: &Path, action: &str, artifact: Option<&Path>) -> bool {
    // Unit-test binaries live in target/{profile}/deps; use the explicitly built
    // operator binary in its parent. No runtime test switches enter production.
    let binary = env::current_exe()
        .unwrap()
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join(if cfg!(windows) {
            "ariang-deploy.exe"
        } else {
            "ariang-deploy"
        });
    let mut command = Command::new(binary);
    command.current_dir(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap(),
    );
    command.arg(action).arg("--config").arg(config);
    if let Some(artifact) = artifact {
        command.arg("--prebuilt").arg(artifact);
    }
    execute(&mut command, None, 180).is_ok()
}

#[test]
#[ignore = "requires Docker Linux containers, OpenSSH, curl and a built operator binary"]
fn disposable_ssh_deployment() {
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_owned();
    let id = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis();
    let private = repo.join(format!(".local/deploy-test-{id}"));
    fs::create_dir_all(&private).unwrap();
    let key = private.join("identity");
    execute(
        Command::new("ssh-keygen")
            .args(["-q", "-t", "ed25519", "-N", "", "-f"])
            .arg(&key),
        None,
        20,
    )
    .unwrap();
    execute(
        Command::new("docker")
            .args(["build", "-t", "ariang-deploy-fixture:local"])
            .arg(repo.join("tests/deploy")),
        None,
        240,
    )
    .unwrap();
    let container = Container(format!("ariang-deploy-test-{id}"));
    execute(
        Command::new("docker")
            .args([
                "run",
                "-d",
                "--name",
                &container.0,
                "-p",
                "127.0.0.1::22",
                "-v",
            ])
            .arg(format!(
                "{}:/fixture-key.pub:ro",
                key.with_extension("pub").display()
            ))
            .arg("ariang-deploy-fixture:local"),
        None,
        30,
    )
    .unwrap();
    let port = output(Command::new("docker").args(["port", &container.0, "22/tcp"]));
    let port = port
        .trim()
        .strip_prefix("127.0.0.1:")
        .expect("Fixture must bind loopback");
    let hostkey = output(Command::new("docker").args([
        "exec",
        &container.0,
        "cat",
        "/etc/ssh/ssh_host_ed25519_key.pub",
    ]));
    let hosts = private.join("known_hosts");
    fs::write(&hosts, format!("[127.0.0.1]:{port} {hostkey}")).unwrap();
    let ssh = private.join("ssh_config");
    fs::write(&ssh, format!("Host disposable\n HostName 127.0.0.1\n Port {port}\n User root\n IdentityFile \"{}\"\n IdentitiesOnly yes\n", key.display().to_string().replace('\\', "/"))).unwrap();
    let config = private.join("deploy.conf");
    let contents = format!(
        "ssh_alias=disposable\nrelease_root=/srv/ariang-release\nhealth_url=http://127.0.0.1:8080/index.html\nknown_hosts={}\nssh_config={}\n",
        hosts.display(),
        ssh.display()
    );
    fs::write(&config, &contents).unwrap();
    let artifact = private.join("artifact");
    fs::create_dir(&artifact).unwrap();
    fs::write(
        artifact.join("LICENSE"),
        fs::read(repo.join("LICENSE")).unwrap(),
    )
    .unwrap();
    fs::write(artifact.join("index.html"), "<html>release A</html>").unwrap();
    let current = || {
        output(Command::new("docker").args([
            "exec",
            &container.0,
            "cat",
            "/srv/ariang-release/current/index.html",
        ]))
    };
    assert!(cli(&config, "dry-run", Some(&artifact)));
    assert_eq!(
        output(Command::new("docker").args([
            "exec",
            &container.0,
            "sh",
            "-c",
            "find /srv/ariang-release -mindepth 1 | wc -l"
        ]))
        .trim(),
        "0"
    );
    assert!(cli(&config, "deploy", Some(&artifact)));
    fs::write(artifact.join("index.html"), "<html>release B</html>").unwrap();
    assert!(cli(&config, "deploy", Some(&artifact)));
    assert!(current().contains("release B"));
    assert!(cli(&config, "rollback", None));
    assert!(current().contains("release A"));

    let failed_build = private.join("failed-build");
    fs::create_dir(&failed_build).unwrap();
    fs::write(
        failed_build.join("package.json"),
        r#"{"name":"fixture","version":"0.0.0","scripts":{"build":"node -e \"process.exit(1)\""}}"#,
    )
    .unwrap();
    fs::write(failed_build.join("package-lock.json"), r#"{"name":"fixture","version":"0.0.0","lockfileVersion":3,"packages":{"":{"name":"fixture","version":"0.0.0"}}}"#).unwrap();
    let binary = env::current_exe()
        .unwrap()
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join(if cfg!(windows) {
            "ariang-deploy.exe"
        } else {
            "ariang-deploy"
        });
    assert!(
        execute(
            Command::new(binary)
                .current_dir(&failed_build)
                .arg("deploy")
                .arg("--config")
                .arg(&config),
            None,
            60
        )
        .is_err(),
        "Build failure must abort deployment"
    );
    assert!(
        current().contains("release A"),
        "Build failure must leave current intact"
    );

    let trusted_hosts = fs::read(&hosts).unwrap();
    fs::write(&hosts, "").unwrap();
    assert!(
        !cli(&config, "deploy", Some(&artifact)),
        "Untrusted host key must fail closed"
    );
    assert!(current().contains("release A"));
    fs::write(&hosts, trusted_hosts).unwrap();

    let bad_health = private.join("bad-health.conf");
    fs::write(
        &bad_health,
        contents.replace("8080/index.html", "8080/missing.html"),
    )
    .unwrap();
    assert!(!cli(&bad_health, "deploy", Some(&artifact)));
    assert!(
        current().contains("release A"),
        "Failed health check must restore old release"
    );

    fs::write(artifact.join("private.local.json"), "{}").unwrap();
    assert!(!cli(&config, "deploy", Some(&artifact)));
    fs::remove_file(artifact.join("private.local.json")).unwrap();
    assert!(current().contains("release A"));
    execute(Command::new("docker").args(["exec", &container.0, "sh", "-c",
        "sed -i 's@Subsystem sftp internal-sftp@Subsystem sftp /bin/false@' /etc/ssh/sshd_config; kill -HUP $(cat /run/sshd.pid)"]), None, 10).unwrap();
    assert!(
        !cli(&config, "deploy", Some(&artifact)),
        "Disabled SFTP must fail transfer"
    );
    assert!(current().contains("release A"));
    execute(Command::new("docker").args(["exec", &container.0, "sh", "-c",
        "sed -i 's@Subsystem sftp /bin/false@Subsystem sftp internal-sftp@' /etc/ssh/sshd_config; kill -HUP $(cat /run/sshd.pid)"]), None, 10).unwrap();
    assert!(
        cli(&config, "deploy", Some(&artifact)),
        "Transfer failure must release its lock"
    );
    assert!(
        cli(&config, "deploy", None),
        "Locked install/build/deploy must succeed"
    );
    assert!(current().contains("AriaNg"));
    println!(
        "Disposable SSH dry-run, artifact rejection, transfer failure, atomic activation, health recovery and rollback passed."
    );
}
