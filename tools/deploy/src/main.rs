use std::{
    collections::BTreeMap,
    env, fs,
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

type Result<T> = std::result::Result<T, &'static str>;
const REMOTE: &str = include_str!("../remote.sh");

#[cfg(test)]
mod ssh_test;

struct Config {
    alias: String,
    root: String,
    health: String,
    known_hosts: PathBuf,
    ssh_config: PathBuf,
}

fn valid_health_url(url: &str) -> bool {
    let Some(rest) = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))
    else {
        return false;
    };
    if url.chars().any(|c| c.is_control() || c.is_whitespace()) || url.contains(['@', '#']) {
        return false;
    }
    let authority = rest.split(['/', '?']).next().unwrap_or("");
    let host = if let Some((host, port)) = authority.rsplit_once(':') {
        if port.parse::<u16>().ok().filter(|port| *port > 0).is_none() {
            return false;
        }
        host
    } else {
        authority
    };
    !host.is_empty()
        && host.split('.').all(|label| {
            !label.is_empty()
                && label.starts_with(|c: char| c.is_ascii_alphanumeric())
                && label.ends_with(|c: char| c.is_ascii_alphanumeric())
                && label
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || c == b'-')
        })
}

fn read_config(path: &Path) -> Result<Config> {
    let text = fs::read_to_string(path).map_err(|_| "Cannot read private config")?;
    let mut values = BTreeMap::new();
    for line in text
        .lines()
        .filter(|line| !line.trim().is_empty() && !line.starts_with('#'))
    {
        let (key, value) = line.split_once('=').ok_or("Expected key=value config")?;
        if ![
            "ssh_alias",
            "release_root",
            "health_url",
            "known_hosts",
            "ssh_config",
        ]
        .contains(&key)
            || value.is_empty()
            || values.insert(key, value).is_some()
        {
            return Err("Unknown, duplicate or empty config value");
        }
    }
    let get = |key| values.get(key).copied().ok_or("Missing config value");
    let config = Config {
        alias: get("ssh_alias")?.to_owned(),
        root: get("release_root")?.to_owned(),
        health: get("health_url")?.to_owned(),
        known_hosts: PathBuf::from(get("known_hosts")?),
        ssh_config: PathBuf::from(get("ssh_config")?),
    };
    if !config
        .alias
        .bytes()
        .all(|c| c.is_ascii_alphanumeric() || b"._-".contains(&c))
        || !config
            .alias
            .starts_with(|c: char| c.is_ascii_alphanumeric())
        || !config.root.starts_with('/')
        || config.root.len() < 2
        || config.root.ends_with('/')
        || config
            .root
            .split('/')
            .skip(1)
            .any(|part| part.is_empty() || part == "." || part == "..")
        || !config
            .root
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"/_-".contains(&c))
        || !valid_health_url(&config.health)
        || !config.known_hosts.is_absolute()
        || !config.ssh_config.is_absolute()
        || !config.known_hosts.is_file()
        || !config.ssh_config.is_file()
    {
        return Err(
            "Invalid config; use an SSH alias, dedicated absolute release root and external SSH files",
        );
    }
    Ok(config)
}

fn execute(command: &mut Command, input: Option<&[u8]>, seconds: u64) -> Result<()> {
    // External tools can echo credentials or infrastructure identifiers. Keep
    // their output private and report the failing phase without command values.
    command.stdout(Stdio::null()).stderr(Stdio::null());
    command.stdin(if input.is_some() {
        Stdio::piped()
    } else {
        Stdio::null()
    });
    let mut child = command.spawn().map_err(|_| "Cannot start required tool")?;
    if let Some(input) = input {
        let mut stdin = child.stdin.take().ok_or("Cannot open tool input")?;
        stdin
            .write_all(input)
            .map_err(|_| "Cannot send tool input")?;
    }
    let deadline = Instant::now() + Duration::from_secs(seconds);
    loop {
        if let Some(status) = child.try_wait().map_err(|_| "Cannot wait for tool")? {
            return if status.success() {
                Ok(())
            } else {
                Err("Tool returned failure")
            };
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err("Tool deadline exceeded");
        }
        thread::sleep(Duration::from_millis(20));
    }
}

fn ssh_options(command: &mut Command, config: &Config) {
    command
        .arg("-F")
        .arg(&config.ssh_config)
        .args([
            "-o",
            "BatchMode=yes",
            "-o",
            "StrictHostKeyChecking=yes",
            "-o",
            "ConnectTimeout=10",
            "-o",
            "ServerAliveInterval=5",
            "-o",
            "ServerAliveCountMax=3",
            "-o",
        ])
        .arg(format!(
            "UserKnownHostsFile={}",
            config.known_hosts.display()
        ));
}

fn quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn remote(config: &Config, action: &str, id: &str) -> Result<()> {
    let mut command = Command::new("ssh");
    ssh_options(&mut command, config);
    command.arg(&config.alias).arg(format!(
        "sh -s -- {} {} {} {}",
        quote(action),
        quote(&config.root),
        quote(id),
        quote(&config.health)
    ));
    execute(&mut command, Some(REMOTE.as_bytes()), 90)
}

fn allowed(relative: &Path) -> bool {
    let name = relative.to_string_lossy().replace('\\', "/");
    if [
        "index.html",
        "LICENSE",
        "robots.txt",
        "favicon.ico",
        "favicon.png",
        "tileicon.png",
        "touchicon.png",
    ]
    .contains(&name.as_str())
    {
        return true;
    }
    let Some((directory, file)) = name.split_once('/') else {
        return false;
    };
    if file.contains('/')
        || file.starts_with('.')
        || !file
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"._-".contains(&c))
    {
        return false;
    }
    match directory {
        "js" => file.ends_with(".js"),
        "css" => file.ends_with(".css"),
        "langs" => file.ends_with(".txt"),
        "fonts" => [".woff", ".woff2", ".ttf", ".eot", ".svg"]
            .iter()
            .any(|ext| file.ends_with(ext)),
        _ => false,
    }
}

fn validate_artifact(root: &Path, directory: &Path, bytes: &mut u64) -> Result<()> {
    for entry in fs::read_dir(directory).map_err(|_| "Cannot read artifact")? {
        let entry = entry.map_err(|_| "Cannot read artifact entry")?;
        let kind = entry.file_type().map_err(|_| "Cannot inspect artifact")?;
        let path = entry.path();
        let relative = path
            .strip_prefix(root)
            .map_err(|_| "Invalid artifact path")?;
        if kind.is_dir() {
            if !["js", "css", "fonts", "langs"]
                .iter()
                .any(|name| relative == Path::new(name))
            {
                return Err("Artifact contains an unexpected directory");
            }
            validate_artifact(root, &path, bytes)?;
        } else if kind.is_file() && allowed(relative) {
            *bytes += entry
                .metadata()
                .map_err(|_| "Cannot inspect artifact size")?
                .len();
            if *bytes > 64 * 1024 * 1024 {
                return Err("Artifact exceeds 64 MiB bound");
            }
        } else {
            return Err("Artifact contains a symlink or unexpected file");
        }
    }
    Ok(())
}

fn artifact(root: &Path) -> Result<()> {
    if fs::symlink_metadata(root)
        .map_err(|_| "Missing artifact")?
        .file_type()
        .is_symlink()
        || !root.join("index.html").is_file()
        || !root.join("LICENSE").is_file()
    {
        return Err("Artifact requires regular index.html and LICENSE files");
    }
    validate_artifact(root, root, &mut 0)
}

fn run() -> Result<()> {
    let args: Vec<String> = env::args().skip(1).collect();
    if args.len() < 3
        || args[1] != "--config"
        || !["deploy", "rollback", "dry-run"].contains(&args[0].as_str())
    {
        return Err(
            "Usage: ariang-deploy <deploy|dry-run|rollback> --config <private-file> [--prebuilt <artifact>]",
        );
    }
    let config = read_config(Path::new(&args[2]))?;
    let prebuilt = match args.get(3).map(String::as_str) {
        None => None,
        Some("--prebuilt") if args.len() == 5 && args[0] != "rollback" => {
            Some(PathBuf::from(&args[4]))
        }
        _ => return Err("Invalid command arguments"),
    };
    if args[0] == "dry-run" {
        if let Some(path) = &prebuilt {
            artifact(path)?;
        }
        println!(
            "Validated private configuration. Plan: build (unless prebuilt), validate static artifact, SSH upload, atomic activation, HTTP content health check; restore previous release on failure. No destination contacted."
        );
        return Ok(());
    }
    let id = format!(
        "r-{}-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| "Invalid clock")?
            .as_nanos(),
        std::process::id()
    );
    if args[0] == "rollback" {
        remote(&config, "rollback", &id)
            .map_err(|_| "Rollback failed; destination values omitted")?;
    } else {
        let root = if let Some(root) = prebuilt {
            root
        } else {
            if !Path::new("package-lock.json").is_file() {
                return Err("Run from the repository root");
            }
            let npm = if cfg!(windows) { "npm.cmd" } else { "npm" };
            execute(
                Command::new(npm).args(["ci", "--no-audit", "--no-fund"]),
                None,
                600,
            )
            .map_err(|_| "Locked dependency installation failed")?;
            execute(Command::new(npm).args(["run", "build"]), None, 180)
                .map_err(|_| "Frontend build failed")?;
            PathBuf::from("dist")
        };
        artifact(&root)?;
        let archive = env::temp_dir().join(format!("ariang-{id}.tar"));
        let result = (|| {
            execute(
                Command::new("tar")
                    .arg("-cf")
                    .arg(&archive)
                    .arg("-C")
                    .arg(&root)
                    .arg("."),
                None,
                30,
            )?;
            remote(&config, "prepare", &id).map_err(|_| "Destination preparation failed")?;
            let mut copy = Command::new("scp");
            ssh_options(&mut copy, &config);
            copy.arg("-q")
                .arg(&archive)
                .arg(format!("{}:{}/{}.tar", config.alias, config.root, id));
            if execute(&mut copy, None, 120).is_err() {
                let _ = remote(&config, "abort", &id);
                return Err("Artifact transfer failed; active release unchanged");
            }
            remote(&config, "activate", &id)
                .map_err(|_| "Activation or health check failed; inspect destination privately")
        })();
        let _ = fs::remove_file(archive);
        result?;
    }
    println!("Operation completed and health check passed; destination values omitted.");
    Ok(())
}

fn main() {
    if let Err(message) = run() {
        eprintln!("{message}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn public_artifact_allowlist() {
        for file in [
            "index.html",
            "LICENSE",
            "js/app.min.js",
            "fonts/font.woff2",
            "langs/en.txt",
        ] {
            assert!(allowed(Path::new(file)));
        }
        for file in [
            ".env",
            "settings.local.json",
            "js/app.js.map",
            "node_modules/x.js",
            "js/../secret.js",
            "js/.secret.js",
            "private.json",
        ] {
            assert!(!allowed(Path::new(file)));
        }
    }
    #[test]
    fn remote_arguments_are_quoted() {
        assert_eq!(quote("a'b"), "'a'\\''b'");
    }

    #[test]
    fn invalid_config_is_rejected_locally() {
        let root = env::temp_dir().join(format!("ariang-config-test-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        let config = root.join("config");
        for text in [
            "",
            "unknown=value",
            "ssh_alias=test\nssh_alias=duplicate",
            "ssh_alias=--option",
        ] {
            fs::write(&config, text).unwrap();
            assert!(read_config(&config).is_err());
        }
        fs::remove_file(config).unwrap();
        fs::remove_dir(root).unwrap();
    }

    #[test]
    fn health_urls_are_validated_before_contact() {
        for url in [
            "https://example.invalid/index.html",
            "http://127.0.0.1:8080/index.html",
        ] {
            assert!(valid_health_url(url));
        }
        for url in [
            "http://",
            "https://:443/index.html",
            "https://example.invalid:99999/",
            "https://user@example.invalid/",
            "https://example..invalid/",
            "file:///index.html",
            "https://example.invalid/#fragment",
        ] {
            assert!(!valid_health_url(url));
        }
    }
}
