# Proton Mail TUI

A text based interface for all the Proton Mail rust crates.

## Disclaimer

**This is an internal testing tool and does not qualify for support or feature
requests**.

## Logging

You can set up custom [logging directives](https://docs.rs/tracing-subscriber/0.3.18/tracing_subscriber/filter/struct.EnvFilter.html) by creating a `log_directives` file.

## Architecture

The setup is using [`ratatui`](https://ratatui.rs/) to render in the terminal
and following the ELM application pattern similar to iced where the UI
reacts to messages.

Note that widget selection for ratatui is not ideal for all cases, so some
ui elements can not be recreated properly. For instance, there is currently
no multi-select list widgets, so we can only operate on a single list item.

### Async

Async tasks should be executed using the `Command::task()` function and
long running background task (e.g.: EventLoop) should be launched via
`Command::background_task()`.

You can customize the interval with --event-loop-time.

## Local Data

Stored in the OS's cache dir for the current user within the folder named
`com.proton.proton-mail-tui`.

### Linux

Default locations unless you have local overwrites.

* ~/.config/com.proton.proton-mail-tui
* ~/.cache/com.proton.proton-mail-tui

### Mac OS

* ~/Library/Caches/com.proton.proton-mail-tui
* ~/Library/Application\ Support/com.proton.proton-mail-tui

## Misc

If `PROTON_OPEN_MESSAGES` is set the messages will be opened in a browser session.
