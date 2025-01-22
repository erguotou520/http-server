use std::fmt;
use std::io::{self, Write};
use std::sync::LazyLock;
use std::thread;

use chrono::{Local, Utc};
use crossbeam_channel::Sender;

// 定义日志级别
#[derive(Debug, Clone, Copy)]
pub enum LogLevel {
    Info = 2,
    Warn = 4,
    Error = 6,
}

impl LogLevel {
    // 将字符串转换为日志级别
    fn from_str<T: AsRef<str>>(str: Option<T>) -> LogLevel {
        match str {
            Some(str) => {
                match str.as_ref().to_lowercase().as_str() {
                    "info" => LogLevel::Info,
                    "warn" => LogLevel::Warn,
                    "error" => LogLevel::Error,
                    _ => LogLevel::Info, // 默认值
                }
            }
            None => LogLevel::Info,
        }
    }
    // 转换为字符串
    fn to_str(&self) -> &'static str {
        match self {
            LogLevel::Info => "INFO",
            LogLevel::Warn => "WARN",
            LogLevel::Error => "ERROR",
        }
    }
}

// 定义日志消息结构
pub struct LogMessage {
    timestamp: chrono::DateTime<Utc>,
    level: LogLevel,
    message: String
}

// 实现日志消息的格式化
impl fmt::Display for LogMessage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let timestamp = self
            .timestamp.with_timezone(&Local).to_rfc3339();
      write!(f, "[{} \x1b[32m{}\x1b[0m] {}", timestamp, self.level.to_str(), self.message)
    }
}

// 日志记录器
pub struct Logger {
    level: LogLevel,
    sender: Sender<LogMessage>,
}

impl Logger {
    // 从环境变量读取日志级别
    pub fn init_from_env() -> Self {
        let log_level = std::env::var("RUST_LOG").ok();
        // let (sender, receiver) = mpsc::channel();
        let (sender, receiver) = crossbeam_channel::unbounded();

        // 启动日志线程
        thread::spawn(move || {
            let mut output = Box::new(io::stdout());

            // 从通道接收日志消息并写入
            for msg in receiver {
                writeln!(output, "{}", msg).expect("Failed to write log");
            }
        });

        Logger {
            sender,
            level: LogLevel::from_str(log_level),
        }
    }

    // 记录日志
    pub fn log(&self, level: LogLevel, message: String) {
        if (level as i32) >= (self.level as i32) {
            let msg = LogMessage {
                level,
                message,
                timestamp: Utc::now(),
            };
            self.sender.send(msg).expect("Failed to send log message");
        }
    }

    // 快捷方法：记录 INFO 日志
    pub fn info(&self, message: String) {
        self.log(LogLevel::Info, message);
    }

    // 快捷方法：记录 WARN 日志
    // pub fn warn(&self, message: String) {
    //     self.log(LogLevel::Warn, message);
    // }

    // // 快捷方法：记录 ERROR 日志
    // pub fn error(&self, message: String) {
    //     self.log(LogLevel::Error, message);
    // }
}

// 日志单例

// 定义日志单例
pub static LOGGER: LazyLock<Logger> = LazyLock::new(|| Logger::init_from_env());
