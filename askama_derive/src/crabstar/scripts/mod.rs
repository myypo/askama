use std::path::Path;
use std::sync::Arc;

use crate::crabstar::CrabstarArgs;
use crate::{CompileError, FileInfo};

const STREAMING_SSR_SCRIPT: &str = include_str!("./streaming-ssr-script.html");
const LIVE_RELOAD_SCRIPT: &str = include_str!("./live-reload-script.html");

fn inject_script(
    content: &mut String,
    script: &str,
    import_from: Option<(&Arc<Path>, &str, &str)>,
) -> Result<(), CompileError> {
    let pos = content.rfind("<!-- crabstar: inject_scripts() -->")
    .or_else(|| content.rfind("</body>"))
    .ok_or_else(|| CompileError::new(
        "page must either contain a visible closing </body> tag or explicitly state where to inject scripts with '<!-- crabstar: inject_scripts() -->' comment",
        import_from.map(|(node_file, file_source, node_source)| {
            FileInfo::new(node_file, Some(file_source), Some(node_source))
        }),
    ))?;

    content.insert_str(pos, script);
    Ok(())
}

pub(crate) fn inject_scripts(
    CrabstarArgs { suspense, page }: &CrabstarArgs,
    source: &mut String,
    import_from: Option<(&Arc<Path>, &str, &str)>,
) -> Result<(), CompileError> {
    if page.is_none() {
        return Ok(());
    }

    if !suspense.is_empty() {
        // TODO: inject it into datastar bundle?
        inject_script(source, STREAMING_SSR_SCRIPT, import_from)?;
    }
    if cfg!(debug_assertions) {
        inject_script(source, LIVE_RELOAD_SCRIPT, import_from)?;
    }

    Ok(())
}
