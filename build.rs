// Puts icon_streamerz.ico into the Windows exe (Explorer, taskbar). No-op on other targets.
fn main() {
    embed_resource::compile("streamerz.rc", embed_resource::NONE)
        .manifest_optional()
        .unwrap();
}
