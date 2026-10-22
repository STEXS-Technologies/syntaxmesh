mod extension;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let arguments = std::env::args().skip(1).collect::<Vec<_>>();
    match arguments.as_slice() {
        #[cfg(feature = "embedded")]
        [] => extension::publish_and_query(),
        [mode] if mode == "--emit-frame" => extension::emit_frame(false),
        [mode] if mode == "--emit-truncated-frame" => extension::emit_frame(true),
        [mode] if mode == "--emit-truncated-frame-success" => {
            extension::emit_truncated_frame_success()
        }
        [mode] if mode == "--sleep" => extension::sleep_forever(),
        [mode, marker] if mode == "--spawn-descendant" => extension::spawn_descendant(marker),
        [mode, marker] if mode == "--write-marker" => extension::write_delayed_marker(marker),
        [mode, marker] if mode == "--write-environment-marker" => {
            extension::write_environment_marker(marker)
        }
        _ => Err("fixture received an unsupported process-conformance mode".into()),
    }
}
