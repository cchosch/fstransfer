use anyhow::Result;
use log4rs::{Config, Handle};
use log4rs::append::console::ConsoleAppender;
use log4rs::config::{Appender, Root};
use log4rs::encode::pattern::PatternEncoder;
use log::LevelFilter;

pub fn init_logger() -> Result<Handle> {
    let pattern_encoder = Box::new(PatternEncoder::new(
        "{d(%H:%M:%S%.3f)} {f}:{L} {h([{l}]):7} {m}{n}",
    ));
    let stdout = ConsoleAppender::builder()
        .encoder(pattern_encoder.clone())
        .build();
    let config = Config::builder()
        .appender(Appender::builder().build("stdout", Box::new(stdout)))
        .build(Root::builder().appender("stdout").build(LevelFilter::Info))?;

    Ok(log4rs::init_config(config)?)
}