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
    let result = match cli.validate_command_options() {
        Err(error) => Err(error),
        Ok(()) => match cli.command.take() {
            Some(Commands::Bump(args)) => verso::release::run_bump(cli, args).map(|()| true),
            Some(Commands::Doctor(args)) => verso::doctor::run_with_status(
                &config_path,
                allow_missing_default_config,
                args.json,
            ),
            Some(Commands::Init(args)) => verso::init::run(&config_path, &args).map(|()| true),
            Some(Commands::Status) => {
                verso::release::transaction_status(&config_path, cli.json).map(|()| true)
            }
            Some(Commands::Resume(args)) => {
                verso::release::resume(&config_path, &args).map(|()| true)
            }
            Some(Commands::Abort) => verso::release::abort(&config_path).map(|()| true),
            None => verso::release::run(cli).map(|()| true),
        },
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
