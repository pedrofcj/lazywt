pub mod collect;
pub mod display;
pub mod json;

use anyhow::Result;

use crate::config::Config;
use crate::layout::ProjectContext;

pub fn run(
    ctx: &ProjectContext,
    _config: &Config,
    short: bool,
    json: bool,
    no_path: bool,
    no_color: bool,
) -> Result<()> {
    let cwd = std::env::current_dir()
        .ok()
        .and_then(|p| dunce::canonicalize(&p).ok());

    let (infos, _default_branch) = collect::collect_all(
        &ctx.project_dir,
        cwd.as_deref(),
        short,
    )?;

    if json {
        self::json::print_json(&infos, &ctx.project_name)?;
    } else {
        display::print_rich(&infos, &ctx.project_name, short, no_path, no_color);
    }

    Ok(())
}
