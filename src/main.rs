use my_lang::input::{SourceContext, Source};
use my_lang::parse::{Lexer, Parser};
use rayon::prelude::*;
use std::env;
use std::path::Path;
use std::sync::Arc;

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    if args.is_empty() {
        eprintln!("Usage: my_lang <file1> <file2> ...");
        std::process::exit(1);
    }

    let ctx = Arc::new(SourceContext::new());

    // Parse all files in parallel and gather results
    let results: Vec<_> = args
        .par_iter()
        .map(|path_str| {
            let path: Arc<Path> = Arc::from(Path::new(path_str));
            let file = ctx.load_file(path.clone());
            let src = Source::File(file.id);

            // Guaranteed initialized OnceCell
            let text_result = file.text.get().expect("file.text not initialized");

            let text = match text_result {
                Ok(s) => s,
                Err(e) => {
                    return Err(format!("Failed to read {}: {}", path.display(), e));
                }
            };

            let lexer = Lexer::new(text, src);
            let mut parser = Parser::new_default(lexer, &ctx);

            match parser.parse_exp() {
                Ok(Some(ast)) => Ok((path, Some(ast))),
                Ok(None) => Ok((path, None)),
                Err(e) => {
                    let mapped = ctx.add_context(e);
                    Err(format!("\nError in {}:\n{}", path.display(), mapped))
                }
            }
        })
        .collect();

    // Print sequentially (to prevent interleaving)
    for result in results {
        match result {
            Ok((path, Some(ast))) => {
                println!("\n=== AST for {} ===", path.display());
                println!("{:#?}", ast);
            }
            Ok((path, None)) => {
                println!("\n(empty parse) {}", path.display());
            }
            Err(msg) => eprintln!("{msg}"),
        }
    }
}
