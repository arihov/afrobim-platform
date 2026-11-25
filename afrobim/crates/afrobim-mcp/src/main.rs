// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! AfroBIM MCP server: lets an AI assistant search the sign catalog, lay out and check signs,
//! write DXFs and build `.afrobim` documents, over the Model Context Protocol on stdio.
//!
//! A port of the C# `SignCore.Mcp` server onto the Rust sign engine, plus document tools the
//! C# server does not have. See the crate README for the tool list and how to register it.

mod documents;
mod engine;
mod paths;
mod registry;
mod rpc;
mod schedule;
mod settings;
mod svg;
#[cfg(test)]
mod testing;
mod tools;

use std::process::ExitCode;

use settings::{Invocation, Settings};

fn main() -> ExitCode {
    let options = match settings::parse_args(std::env::args().skip(1)) {
        Ok(Invocation::Serve(options)) => options,
        Ok(Invocation::Help) => {
            println!("{}", settings::USAGE);
            return ExitCode::SUCCESS;
        }
        Ok(Invocation::Version) => {
            println!("afrobim-mcp {}", rpc::SERVER_VERSION);
            return ExitCode::SUCCESS;
        }
        Err(e) => {
            eprintln!("afrobim-mcp: {e}\n\n{}", settings::USAGE);
            return ExitCode::from(2);
        }
    };

    // Standard output carries JSON-RPC only, so everything else goes to standard error. A
    // missing pack or fonts folder is not fatal: the tools that need it say what to fix.
    let settings = match Settings::from_environment(&options) {
        Ok(settings) => settings,
        Err(e) => {
            eprintln!("afrobim-mcp: {e}");
            return ExitCode::from(2);
        }
    };
    for line in settings.startup_log() {
        eprintln!("afrobim-mcp: {line}");
    }
    for problem in [
        settings.pack_file.as_ref().err(),
        settings.fonts_dir.as_ref().err(),
    ]
    .into_iter()
    .flatten()
    {
        eprintln!("afrobim-mcp: warning: {problem}");
    }

    let mut server = rpc::Server::new(tools::Tools::new(settings));
    let (stdin, stdout) = (std::io::stdin(), std::io::stdout());
    match server.serve(stdin.lock(), stdout.lock()) {
        Ok(()) => ExitCode::SUCCESS,
        // The client went away: nothing left to do.
        Err(e) if e.kind() == std::io::ErrorKind::BrokenPipe => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("afrobim-mcp: {e}");
            ExitCode::FAILURE
        }
    }
}
