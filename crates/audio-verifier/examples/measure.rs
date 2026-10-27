use std::time::Instant;

fn main() {
    let repeat: usize = std::env::var("MEASURE_REPEAT")
        .unwrap_or_else(|_| "1".into())
        .parse()
        .unwrap();
    assert!(repeat > 0);
    for path in std::env::args().skip(1) {
        let bytes = std::fs::read(&path).expect("input");
        let mut times = Vec::new();
        let mut answer = None;
        for _ in 0..repeat {
            let start = Instant::now();
            let result = audio_verifier::measure(&bytes);
            times.push(start.elapsed().as_secs_f64());
            let value = match result {
                Ok(payload) => serde_json::json!({"accepted": true, "payload": payload}),
                Err(error) => serde_json::json!({"accepted": false, "error": error.to_string()}),
            };
            if let Some(previous) = &answer {
                assert_eq!(previous, &value);
            }
            answer = Some(value);
        }
        println!(
            "{}",
            serde_json::json!({"file": path, "bytes": bytes.len(), "result": answer, "seconds": times})
        );
    }
}
