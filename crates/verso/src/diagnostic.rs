use anstyle::{AnsiColor, Style};
use std::{
    env,
    io::{self, IsTerminal},
};

pub fn render_error(message: &str, styled: bool) -> String {
    match message.strip_prefix("cancelled: ") {
        Some(message) => render("cancelled", message, cancelled_style(), styled),
        None => render("error", message, error_style(), styled),
    }
}

pub fn render_warning(message: &str, styled: bool) -> String {
    render("warning", message, warning_style(), styled)
}

pub fn render_check(passed: bool, name: &str, message: &str, styled: bool) -> String {
    let (label, label_style) = if passed {
        ("PASS", success_style())
    } else {
        ("FAIL", error_style())
    };
    let mut output = String::new();
    let mut lines = message.lines();
    let first = lines.next().unwrap_or_default();

    output.push_str(&styled_text(label, label_style, styled));
    output.push_str(&format!("  {name:<16} {first}\n"));
    for line in lines {
        if line.is_empty() {
            output.push('\n');
        } else if let Some(message) = line.strip_prefix("help: ") {
            push_labeled_line(&mut output, "      ", "help", message, help_style(), styled);
        } else if let Some(message) = line.strip_prefix("note: ") {
            push_labeled_line(&mut output, "      ", "note", message, note_style(), styled);
        } else {
            output.push_str("      ");
            output.push_str(line);
            output.push('\n');
        }
    }

    output
}

pub fn stdout_supports_color() -> bool {
    supports_color(io::stdout().is_terminal())
}

pub fn stderr_supports_color() -> bool {
    supports_color(io::stderr().is_terminal())
}

fn supports_color(is_terminal: bool) -> bool {
    is_terminal
        && env::var_os("NO_COLOR").is_none()
        && env::var_os("TERM").is_none_or(|term| term != "dumb")
}

fn render(label: &str, message: &str, label_style: Style, styled: bool) -> String {
    let mut output = String::new();
    let mut lines = message.lines();
    let first = lines.next().unwrap_or_default();
    push_labeled_line(&mut output, "", label, first, label_style, styled);

    for line in lines {
        if line.is_empty() {
            output.push('\n');
        } else if let Some(message) = line.strip_prefix("help: ") {
            push_labeled_line(&mut output, "", "help", message, help_style(), styled);
        } else if let Some(message) = line.strip_prefix("note: ") {
            push_labeled_line(&mut output, "", "note", message, note_style(), styled);
        } else {
            output.push_str("  ");
            output.push_str(line);
            output.push('\n');
        }
    }

    output
}

fn push_labeled_line(
    output: &mut String,
    indent: &str,
    label: &str,
    message: &str,
    label_style: Style,
    styled: bool,
) {
    output.push_str(indent);
    output.push_str(&styled_text(label, label_style, styled));
    output.push_str(": ");
    output.push_str(message);
    output.push('\n');
}

fn styled_text(value: &str, style: Style, styled: bool) -> String {
    if styled {
        format!("{}{value}\u{1b}[0m", style.render())
    } else {
        value.to_owned()
    }
}

fn error_style() -> Style {
    Style::new().bold().fg_color(Some(AnsiColor::Red.into()))
}

fn warning_style() -> Style {
    Style::new().bold().fg_color(Some(AnsiColor::Yellow.into()))
}

fn success_style() -> Style {
    Style::new().bold().fg_color(Some(AnsiColor::Green.into()))
}

fn cancelled_style() -> Style {
    Style::new().fg_color(Some(AnsiColor::Yellow.into()))
}

fn help_style() -> Style {
    Style::new().fg_color(Some(AnsiColor::Cyan.into()))
}

fn note_style() -> Style {
    Style::new().dimmed()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_error_preserves_semantic_labels() {
        let output = render_error(
            "release failed\n\nnote: local tag was kept\nhelp: push it manually",
            false,
        );

        assert_eq!(
            output,
            "error: release failed\n\nnote: local tag was kept\nhelp: push it manually\n"
        );
        assert!(!output.contains("\u{1b}["));
    }

    #[test]
    fn styled_error_colors_labels() {
        let output = render_error("release failed\nhelp: retry", true);

        assert!(output.contains("\u{1b}["));
        assert!(output.contains("error\u{1b}[0m: release failed"));
        assert!(output.contains("help\u{1b}[0m: retry"));
    }

    #[test]
    fn cancellation_is_not_rendered_as_an_error() {
        let output = render_error("cancelled: release aborted", false);

        assert_eq!(output, "cancelled: release aborted\n");
    }

    #[test]
    fn styled_check_colors_the_status_without_losing_plain_labels() {
        let output = render_check(false, "packages", "missing version", true);

        assert!(output.contains("FAIL\u{1b}[0m  packages"));
        assert!(output.contains("missing version"));
    }
}
