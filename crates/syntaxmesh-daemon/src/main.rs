mod host;
#[cfg(feature = "inference-sqlite")]
mod inference_config;
mod options;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    if let Some(options) = options::parse(std::env::args_os().skip(1))? {
        host::run(options)?;
    } else {
        println!("{}", options::USAGE);
    }
    Ok(())
}
