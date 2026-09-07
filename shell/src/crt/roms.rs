//! Per system view of the library for the CLI: folder or index, counts,
//! files with unknown extensions, core presence.

use crate::library::{Library, expand};

pub struct Scan {
    pub name: String,
    pub dir: String,
    pub exists: bool,
    pub games: usize,
    pub unknown: usize,
    pub core_present: bool,
}

/// One row per system: folder, game count, files with unknown extensions,
/// whether the core is installed.
pub fn scan(lib: &Library) -> Vec<Scan> {
    lib.systems
        .iter()
        .map(|s| {
            let dir = expand(&s.dir);
            let indexed = lib.uses_index(s);
            let exists = indexed || dir.is_dir();
            let games = lib.count(s);
            let unknown = if !indexed && exists && !s.extensions.is_empty() {
                std::fs::read_dir(&dir)
                    .map(|rd| {
                        rd.filter_map(|e| e.ok())
                            .filter(|e| e.file_type().map(|t| t.is_file()).unwrap_or(false))
                            .filter(|e| {
                                let p = e.path();
                                let ext = p.extension().and_then(|x| x.to_str()).unwrap_or("");
                                !s.extensions.iter().any(|x| x.eq_ignore_ascii_case(ext))
                                    && !ext.eq_ignore_ascii_case("m3u")
                            })
                            .count()
                    })
                    .unwrap_or(0)
            } else {
                0
            };
            let core_present = s.is_video() || lib.core_path(s).exists();
            Scan {
                name: s.name.clone(),
                dir: if indexed {
                    "index".to_string()
                } else {
                    dir.display().to_string()
                },
                exists,
                games,
                unknown,
                core_present,
            }
        })
        .collect()
}
