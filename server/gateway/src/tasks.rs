use super::*;
use std::cmp::Ordering;

const MAX_TASKS: usize = 1000;
const KEYS: &[&str] = &[
    "gid",
    "status",
    "totalLength",
    "completedLength",
    "uploadLength",
    "downloadSpeed",
    "uploadSpeed",
    "connections",
    "numSeeders",
    "seeder",
    "errorCode",
    "verifiedLength",
    "verifyIntegrityPending",
    "infoHash",
    "files",
    "bittorrent",
];

pub(super) fn source(method: &str) -> Option<&'static str> {
    match method {
        "gateway.tasks.active" => Some("aria2.tellActive"),
        "gateway.tasks.waiting" => Some("aria2.tellWaiting"),
        "gateway.tasks.stopped" => Some("aria2.tellStopped"),
        _ => None,
    }
}

pub(super) fn upstream_params(source: &str) -> Vec<Value> {
    if source == "aria2.tellActive" {
        vec![json!(KEYS)]
    } else if source == "aria2.tellWaiting" {
        vec![json!(0), json!(MAX_TASKS + 1), json!(KEYS)]
    } else {
        vec![json!(-1), json!(MAX_TASKS + 1), json!(KEYS)]
    }
}

pub(super) fn compact(value: Value) -> Result<Value, Fault> {
    let rows = value.as_array().ok_or(UNAVAILABLE)?;
    if rows.len() > MAX_TASKS {
        return Err(UNAVAILABLE);
    }
    let mut gids = std::collections::HashSet::new();
    let mut result = Vec::with_capacity(rows.len());
    for row in rows {
        let gid = row["gid"]
            .as_str()
            .filter(|s| s.len() == 16 && s.bytes().all(|b| b.is_ascii_hexdigit()))
            .ok_or(UNAVAILABLE)?;
        if !gids.insert(gid) || row["status"].as_str().is_none() {
            return Err(UNAVAILABLE);
        }
        for key in [
            "totalLength",
            "completedLength",
            "downloadSpeed",
            "uploadSpeed",
        ] {
            row[key]
                .as_str()
                .and_then(|s| s.parse::<u64>().ok())
                .ok_or(UNAVAILABLE)?;
        }
        let files = row["files"].as_array().ok_or(UNAVAILABLE)?;
        let name = row
            .pointer("/bittorrent/info/name")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
            .or_else(|| files.first().and_then(file_name));
        let mut summary = serde_json::Map::new();
        for key in KEYS
            .iter()
            .filter(|k| !matches!(**k, "files" | "bittorrent"))
        {
            if let Some(value) = row.get(*key) {
                summary.insert((*key).into(), value.clone());
            }
        }
        summary.insert(
            "taskName".into(),
            name.map(Value::String).unwrap_or(Value::Null),
        );
        summary.insert("fileCount".into(), json!(files.len()));
        summary.insert(
            "isBitTorrent".into(),
            json!(row.get("bittorrent").is_some()),
        );
        if files.len() == 1 {
            if let Some(uris) = files[0]["uris"].as_array() {
                if let Some(uri) = uris.first().and_then(|u| u["uri"].as_str()) {
                    if uris.iter().all(|u| u["uri"].as_str() == Some(uri)) {
                        summary.insert("singleUrl".into(), json!(uri));
                    }
                }
            }
        }
        summary.insert(
            "selectedFileCount".into(),
            json!(files.iter().filter(|f| f["selected"] == "true").count()),
        );
        result.push(Value::Object(summary));
    }
    Ok(Value::Array(result))
}

fn file_name(file: &Value) -> Option<String> {
    let path = file["path"].as_str().filter(|s| !s.is_empty());
    let from_uri = path.is_none();
    let path = path.or_else(|| file.pointer("/uris/0/uri").and_then(Value::as_str))?;
    let name = match path.rfind('/') {
        Some(i) if i > 0 => path[i + 1..]
            .split_once('?')
            .filter(|(name, _)| !name.is_empty())
            .map_or(&path[i + 1..], |(name, _)| name),
        _ => path,
    };
    let name = if from_uri {
        decode_uri(name)
    } else {
        name.to_owned()
    };
    (!name.is_empty()).then_some(name)
}

// Match decodeURI rather than decodeURIComponent: reserved URI characters remain
// escaped and a malformed escape/UTF-8 sequence leaves the whole name unchanged.
fn decode_uri(input: &str) -> String {
    let mut result = Vec::with_capacity(input.len());
    let bytes = input.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let Some(pair) = input.get(i + 1..i + 3) else {
                return input.into();
            };
            let Ok(byte) = u8::from_str_radix(pair, 16) else {
                return input.into();
            };
            if b";/?:@&=+$,#".contains(&byte) {
                result.extend_from_slice(&bytes[i..i + 3]);
            } else {
                result.push(byte);
            }
            i += 3;
        } else {
            result.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(result).unwrap_or_else(|_| input.into())
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "lowercase")]
enum Source {
    #[default]
    Active,
    Waiting,
    Stopped,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "lowercase")]
enum Sort {
    #[default]
    Position,
    Name,
    Size,
    Percent,
    Remain,
    Dspeed,
    Uspeed,
}

#[derive(Deserialize)]
#[serde(default, deny_unknown_fields)]
struct Query {
    source: Source,
    search: String,
    sort: Sort,
    reverse: bool,
    offset: usize,
    limit: usize,
    snapshot: Option<String>,
}

impl Default for Query {
    fn default() -> Self {
        Self {
            source: Source::default(),
            search: String::new(),
            sort: Sort::default(),
            reverse: false,
            offset: 0,
            limit: 100,
            snapshot: None,
        }
    }
}

fn number(row: &Value, key: &str) -> u64 {
    row[key].as_str().and_then(|s| s.parse().ok()).unwrap_or(0)
}

fn remaining(row: &Value) -> u64 {
    number(row, "totalLength").saturating_sub(number(row, "completedLength"))
}

fn compare(a: &Value, b: &Value, sort: &Sort) -> Ordering {
    let ratio = |row: &Value| {
        let total = number(row, "totalLength");
        if total == 0 {
            if row["status"] == "complete" {
                1.0
            } else {
                0.0
            }
        } else {
            number(row, "completedLength") as f64 / total as f64
        }
    };
    let eta = |row: &Value| {
        let speed = number(row, "downloadSpeed");
        if speed == 0 {
            0.0
        } else {
            remaining(row) as f64 / speed as f64
        }
    };
    match sort {
        Sort::Position => Ordering::Equal,
        Sort::Name => a["taskName"]
            .as_str()
            .unwrap_or("")
            .to_lowercase()
            .cmp(&b["taskName"].as_str().unwrap_or("").to_lowercase()),
        Sort::Size => number(a, "totalLength").cmp(&number(b, "totalLength")),
        Sort::Percent => ratio(a).total_cmp(&ratio(b)),
        Sort::Dspeed => number(a, "downloadSpeed").cmp(&number(b, "downloadSpeed")),
        Sort::Uspeed => number(a, "uploadSpeed").cmp(&number(b, "uploadSpeed")),
        Sort::Remain => (number(a, "downloadSpeed") == 0)
            .cmp(&(number(b, "downloadSpeed") == 0))
            .then_with(|| eta(a).total_cmp(&eta(b)))
            .then_with(|| remaining(a).cmp(&remaining(b))),
    }
}

impl Gateway {
    pub(super) async fn dispatch(
        &self,
        scope: &Scope,
        method: &str,
        params: Vec<Value>,
    ) -> Result<(Arc<Value>, u128), Fault> {
        if method != "gateway.listTasks" {
            return self.call(scope, method, params).await;
        }
        if params.len() > 1 {
            return Err(INVALID);
        }
        let query: Query =
            serde_json::from_value(params.into_iter().next().unwrap_or_else(|| json!({})))
                .map_err(|_| INVALID)?;
        if query.limit == 0
            || query.limit > 200
            || query.offset > MAX_TASKS
            || query.search.len() > 256
            || query.snapshot.as_ref().is_some_and(|s| s.len() != 64)
        {
            return Err(INVALID);
        }
        let method = match query.source {
            Source::Active => "gateway.tasks.active",
            Source::Waiting => "gateway.tasks.waiting",
            Source::Stopped => "gateway.tasks.stopped",
        };
        let (snapshot, age) = self.call(scope, method, vec![]).await?;
        let bytes = serde_json::to_vec(snapshot.as_ref()).map_err(|_| UNAVAILABLE)?;
        let mut hash = Sha256::new();
        hash.update(scope.config.id.as_bytes());
        hash.update(method.as_bytes());
        hash.update(bytes);
        let revision = format!("{:x}", hash.finalize());
        if query.snapshot.as_ref().is_some_and(|s| *s != revision) {
            return Err(Fault(StatusCode::CONFLICT, -32003, "Snapshot expired"));
        }
        let rows = snapshot.as_array().ok_or(UNAVAILABLE)?;
        let search = query.search.to_lowercase();
        let mut filtered: Vec<_> = rows
            .iter()
            .enumerate()
            .filter(|(_, row)| {
                row["taskName"]
                    .as_str()
                    .unwrap_or("")
                    .to_lowercase()
                    .contains(&search)
            })
            .collect();
        filtered.sort_by(|(ai, a), (bi, b)| {
            let order = if matches!(query.sort, Sort::Position) {
                ai.cmp(bi)
            } else {
                compare(a, b, &query.sort)
            };
            let order = if query.reverse {
                order.reverse()
            } else {
                order
            };
            order.then_with(|| ai.cmp(bi))
        });
        let total = filtered.len();
        let page: Vec<_> = filtered
            .into_iter()
            .skip(query.offset)
            .take(query.limit)
            .map(|(position, row)| {
                let mut row = row.clone();
                row["position"] = json!(position);
                row
            })
            .collect();
        Ok((
            Arc::new(
                json!({"snapshot":revision,"sourceCount":rows.len(),"total":total,"offset":query.offset,"limit":query.limit,
            "hasMore":query.offset.saturating_add(page.len()) < total,"tasks":page}),
            ),
            age,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn task() -> Value {
        json!({"gid":"0000000000000001","status":"complete","totalLength":"0","completedLength":"0",
            "downloadSpeed":"0","uploadSpeed":"0","files":[{"path":"","selected":"true","uris":[{"uri":"https://example.invalid/a%20b%2Fc?query"}]}]})
    }

    #[test]
    fn summary_names_prefer_torrents_and_preserve_uri_decode_semantics() {
        let mut row = task();
        let summary = compact(json!([row])).unwrap();
        assert_eq!(summary[0]["taskName"], "a b%2Fc");
        assert_eq!(summary[0]["isBitTorrent"], false);
        assert_eq!(
            summary[0]["singleUrl"],
            task()["files"][0]["uris"][0]["uri"]
        );
        row["bittorrent"] = json!({"info":{"name":"torrent"}});
        let summary = compact(json!([row])).unwrap();
        assert_eq!(summary[0]["taskName"], "torrent");
        assert_eq!(summary[0]["isBitTorrent"], true);
        let mut row = task();
        row["files"][0]["uris"] =
            json!([{"uri":"https://example.invalid/a"}, {"uri":"https://example.invalid/b"}]);
        assert!(compact(json!([row])).unwrap()[0].get("singleUrl").is_none());
        let mut row = task();
        row["files"][0]["uris"][0]["uri"] = json!("https://example.invalid/bad%FF");
        assert_eq!(compact(json!([row])).unwrap()[0]["taskName"], "bad%FF");
        let mut row = task();
        row["files"] = json!([]);
        assert!(compact(json!([row])).unwrap()[0]["taskName"].is_null());
    }

    #[test]
    fn malformed_duplicate_and_oversized_inventories_fail_without_truncation() {
        let row = task();
        assert!(compact(json!([row, row])).is_err());
        assert!(compact(json!({})).is_err());
        let mut invalid = task();
        invalid["downloadSpeed"] = json!("invalid");
        assert!(compact(json!([invalid])).is_err());
        assert!(
            compact(Value::Array(
                (0..1001)
                    .map(|i| {
                        let mut row = task();
                        row["gid"] = json!(format!("{i:016x}"));
                        row
                    })
                    .collect()
            ))
            .is_err()
        );
    }

    #[test]
    fn numeric_and_eta_sorts_match_task_values_including_zero_length_completion() {
        let complete = task();
        let mut active = task();
        active["status"] = json!("active");
        assert_eq!(
            compare(&complete, &active, &Sort::Percent),
            Ordering::Greater
        );
        active["totalLength"] = json!("100");
        active["downloadSpeed"] = json!("10");
        assert_eq!(compare(&active, &complete, &Sort::Remain), Ordering::Less);
        let mut smaller = active.clone();
        smaller["totalLength"] = json!("9");
        assert_eq!(compare(&smaller, &active, &Sort::Size), Ordering::Less);
    }
}
