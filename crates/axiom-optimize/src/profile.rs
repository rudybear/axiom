use std::collections::HashMap;

/// A hot function identified from execution trace data.
#[derive(Debug, Clone)]
pub struct HotFunction {
    pub name: String,
    pub call_count: u64,
    pub total_ns: u64,
    pub percent_of_runtime: f64,
}

/// Parsed profile data from .trace.jsonl execution traces.
#[derive(Debug, Clone)]
pub struct ProfileData {
    pub hot_functions: Vec<HotFunction>,
    pub total_runtime_ns: u64,
    pub call_graph: Vec<(String, String, u64)>,  // (caller, callee, count)
}

/// Parse a .trace.jsonl file into structured profile data.
pub fn parse_trace(jsonl_path: &str) -> Result<ProfileData, String> {
    let content = std::fs::read_to_string(jsonl_path)
        .map_err(|e| format!("failed to read trace file: {e}"))?;

    let mut func_times: HashMap<String, (u64, u64)> = HashMap::new(); // name -> (total_ns, count)
    let mut call_stack: Vec<(String, u64)> = Vec::new(); // (func_name, enter_ns)
    let mut call_edges: HashMap<(String, String), u64> = HashMap::new();
    let mut min_ns: u64 = u64::MAX;
    let mut max_ns: u64 = 0;

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() { continue; }

        // Minimal JSON parsing (no serde dependency)
        let event_type = extract_json_str(line, "type").unwrap_or_default();
        let func = extract_json_str(line, "func").unwrap_or_default();
        let ns = extract_json_num(line, "ns").unwrap_or(0);

        if ns < min_ns { min_ns = ns; }
        if ns > max_ns { max_ns = ns; }

        match event_type.as_str() {
            "enter" => {
                // Record call edge from parent
                if let Some((parent, _)) = call_stack.last() {
                    *call_edges.entry((parent.clone(), func.clone())).or_insert(0) += 1;
                }
                call_stack.push((func, ns));
            }
            "exit" => {
                if let Some((name, enter_ns)) = call_stack.pop() {
                    if name == func && ns >= enter_ns {
                        let duration = ns - enter_ns;
                        let entry = func_times.entry(name).or_insert((0, 0));
                        entry.0 += duration;
                        entry.1 += 1;
                    }
                }
            }
            _ => {}
        }
    }

    let total_runtime = if max_ns > min_ns { max_ns - min_ns } else { 1 };

    let mut hot_functions: Vec<HotFunction> = func_times
        .into_iter()
        .map(|(name, (total_ns, count))| HotFunction {
            percent_of_runtime: (total_ns as f64 / total_runtime as f64) * 100.0,
            name,
            call_count: count,
            total_ns,
        })
        .collect();

    hot_functions.sort_by(|a, b| b.total_ns.cmp(&a.total_ns));

    let mut call_graph: Vec<(String, String, u64)> = call_edges
        .into_iter()
        .map(|((from, to), count)| (from, to, count))
        .collect();
    call_graph.sort_by(|a, b| b.2.cmp(&a.2));

    Ok(ProfileData {
        hot_functions,
        total_runtime_ns: total_runtime,
        call_graph,
    })
}

fn extract_json_str(json: &str, key: &str) -> Option<String> {
    let pattern = format!("\"{}\":\"", key);
    let start = json.find(&pattern)? + pattern.len();
    let end = json[start..].find('"')? + start;
    Some(json[start..end].to_string())
}

fn extract_json_num(json: &str, key: &str) -> Option<u64> {
    let pattern = format!("\"{}\":", key);
    let start = json.find(&pattern)? + pattern.len();
    let rest = json[start..].trim_start();
    let end = rest.find(|c: char| !c.is_ascii_digit()).unwrap_or(rest.len());
    rest[..end].parse().ok()
}

/// Format profile data as a string for inclusion in LLM prompts.
pub fn format_profile_for_prompt(profile: &ProfileData) -> String {
    let mut out = String::new();
    out.push_str("## Runtime Profile Data\n\n");
    out.push_str("Hot functions (sorted by runtime):\n");
    for (i, f) in profile.hot_functions.iter().take(10).enumerate() {
        out.push_str(&format!(
            "{}. {}: {} calls, {:.1}% of runtime ({:.3}ms)\n",
            i + 1, f.name, f.call_count, f.percent_of_runtime,
            f.total_ns as f64 / 1_000_000.0
        ));
    }
    if !profile.call_graph.is_empty() {
        out.push_str("\nCall graph (top edges):\n");
        for (from, to, count) in profile.call_graph.iter().take(10) {
            out.push_str(&format!("- {} \u{2192} {}: {} calls\n", from, to, count));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_json_str() {
        let line = r#"{"type":"enter","func":"main","ns":12345}"#;
        assert_eq!(extract_json_str(line, "type"), Some("enter".to_string()));
        assert_eq!(extract_json_str(line, "func"), Some("main".to_string()));
    }

    #[test]
    fn test_extract_json_num() {
        let line = r#"{"type":"enter","func":"main","ns":12345}"#;
        assert_eq!(extract_json_num(line, "ns"), Some(12345));
    }

    #[test]
    fn test_parse_trace_basic() {
        let tmpdir = std::env::temp_dir();
        let path = tmpdir.join("test_trace.jsonl");
        std::fs::write(&path, r#"{"type":"enter","func":"main","ns":1000}
{"type":"enter","func":"work","ns":2000}
{"type":"exit","func":"work","ns":5000}
{"type":"exit","func":"main","ns":6000}
"#).unwrap();
        let profile = parse_trace(path.to_str().unwrap()).unwrap();
        assert_eq!(profile.hot_functions.len(), 2);
        assert_eq!(profile.hot_functions[0].name, "main"); // main has longer duration
        assert_eq!(profile.hot_functions[1].name, "work");
        assert_eq!(profile.hot_functions[1].total_ns, 3000);
        assert_eq!(profile.hot_functions[1].call_count, 1);
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn test_format_profile() {
        let profile = ProfileData {
            hot_functions: vec![
                HotFunction { name: "hot".to_string(), call_count: 1000, total_ns: 5000000, percent_of_runtime: 50.0 },
                HotFunction { name: "warm".to_string(), call_count: 100, total_ns: 3000000, percent_of_runtime: 30.0 },
            ],
            total_runtime_ns: 10000000,
            call_graph: vec![("main".to_string(), "hot".to_string(), 1000)],
        };
        let text = format_profile_for_prompt(&profile);
        assert!(text.contains("hot: 1000 calls"));
        assert!(text.contains("50.0%"));
        assert!(text.contains("main"));
        assert!(text.contains("hot"));
    }
}
