use std::io::{self, Write};

use workspace_provider_boat::{Notice, Warnings};

pub struct StderrWarnings;

impl Warnings for StderrWarnings {
    fn warning(&self, notice: Notice) {
        let _ = writeln!(io::stderr(), "warning: {notice}");
    }
}
