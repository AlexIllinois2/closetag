use std::fs;
use std::path::Path;

fn main() {
    // rust-embed requires the folder to exist at compile time.
    // If the frontend has not been built yet, drop a placeholder index.html so
    // `cargo build` still succeeds; run `bun run build` in frontend/ (or
    // scripts/build.sh) to embed the real UI.
    let dist = Path::new("frontend/dist");
    if !dist.exists() {
        fs::create_dir_all(dist).expect("create frontend/dist");
    }
    let index = dist.join("index.html");
    if !index.exists() {
        fs::write(
            &index,
            "<!doctype html><html><head><meta charset=\"utf-8\"><title>closetag</title></head>\
             <body style=\"font-family:system-ui,sans-serif;padding:3rem;line-height:1.6\">\
             <h1>closetag</h1>\
             <p>Frontend assets are not embedded in this build.</p>\
             <p>Run <code>bun install &amp;&amp; bun run build</code> inside <code>frontend/</code>\
             (or <code>scripts/build.sh</code>), then <code>cargo build --release</code> again.</p>\
             </body></html>",
        )
        .expect("write placeholder index.html");
        println!("cargo:warning=frontend/dist not built - embedded placeholder page only");
    }
    println!("cargo:rerun-if-changed=frontend/dist");
}
