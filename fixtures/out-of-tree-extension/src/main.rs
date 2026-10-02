mod extension;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let arguments = std::env::args().skip(1).collect::<Vec<_>>();
    match arguments.as_slice() {
        #[cfg(feature = "embedded")]
        [] => extension::publish_and_query(),
        [mode] if mode == "--emit-frame" => extension::emit_frame(false),
        [mode] if mode == "--emit-truncated-frame" => extension::emit_frame(true),
        _ => Err("fixture accepts --emit-frame or --emit-truncated-frame".into()),
    }
}
