use verso::cli::{Cli, Commands};
use verso::diagnostic::{render_error, stderr_supports_color};

fn main() {
    let mut cli = Cli::parse_args();
    if cli.tool_version {
        println!("{}", env!("CARGO_PKG_VERSION"));
        return;
    }

    let config_path = cli.config_path_buf();
    let allow_missing_default_config = !cli.config_was_explicit();
    let command = cli.command.take();
    let result = match command {
        Some(Commands::Doctor(args)) => {
            verso::doctor::run_with_status(&config_path, allow_missing_default_config, args.json)
        }
        Some(Commands::Init(args)) => verso::init::run(&config_path, &args).map(|()| true),
        None => verso::release::run(cli).map(|()| true),
    };

    match result {
        Ok(true) => {}
        Ok(false) => std::process::exit(1),
        Err(error) => {
            eprint!("{}", render_error(&error, stderr_supports_color()));
            std::process::exit(1);
        }
    }
}
