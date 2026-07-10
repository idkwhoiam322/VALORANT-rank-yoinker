use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;

use chrono::Local;

pub struct Logger {
    log_path: PathBuf,
    buffer: Mutex<Vec<String>>,
    max_buffer: usize,
}

impl Logger {
    pub fn new(root: PathBuf) -> Self {
        let logs_dir = root.join("logs");
        let _ = fs::create_dir_all(&logs_dir);

        let run_num = Self::next_run_num(&logs_dir);
        let log_path = logs_dir.join(format!("log-{}.txt", run_num));

        let logger = Self {
            log_path,
            buffer: Mutex::new(Vec::with_capacity(100)),
            max_buffer: 500,
        };

        logger.log("Logger initialized");
        logger
    }

    fn next_run_num(dir: &PathBuf) -> u32 {
        let mut max = 0u32;
        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.flatten() {
                let name = entry.file_name();
                let name_str = name.to_string_lossy();
                if name_str.starts_with("log-") && name_str.ends_with(".txt") {
                    if let Some(num_str) = name_str.strip_prefix("log-").and_then(|s| s.strip_suffix(".txt")) {
                        if let Ok(num) = num_str.parse::<u32>() {
                            if num > max {
                                max = num;
                            }
                        }
                    }
                }
            }
        }
        max + 1
    }

    pub fn log(&self, message: &str) {
        let timestamp = Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
        let line = format!("[{}] {}", timestamp, message);

        let mut buffer = self.buffer.lock().unwrap();
        buffer.push(line.clone());
        while buffer.len() > self.max_buffer {
            buffer.remove(0);
        }

        // Always write to file immediately
        if let Ok(mut file) = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.log_path)
        {
            let _ = writeln!(file, "{}", line);
        }

        #[cfg(debug_assertions)]
        println!("{}", line);
    }

    pub fn get_tail(&self, count: usize) -> String {
        let buffer = self.buffer.lock().unwrap();
        let start = buffer.len().saturating_sub(count);
        buffer[start..].join("\n")
    }
}
