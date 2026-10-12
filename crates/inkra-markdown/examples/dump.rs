//! Dumps the AST of a Markdown file or stdin: `cargo run --example dump -- file.md`.
fn main() {
    let src = match std::env::args().nth(1) {
        Some(p) => std::fs::read_to_string(p).expect("read"),
        None => {
            let mut s = String::new();
            std::io::Read::read_to_string(&mut std::io::stdin(), &mut s).expect("stdin");
            s
        }
    };
    let p = inkra_markdown::parse(src.as_str(), &inkra_markdown::ParseOptions::default());
    println!("{:#?}", p.blocks);
}
