//! The groups a dead-code list can be filtered by, taken from the folders the
//! candidates are really in. Nothing here knows a folder name: a project with
//! `website/` and `app/` gets those, one with `lib/` and `cmd/` gets those, and
//! a single script in the project root gets no filter at all.

use std::collections::HashMap;

/// The area of a file that sits in the project root. A folder can never be
/// named `.`, so it cannot clash with a real one.
pub(super) const ROOT: &str = ".";

/// How many folders deep an area name goes at most. Generous on purpose: a
/// project can keep everything under a long common path (`app/src/main/java/...`)
/// and the areas only get told apart below it.
const MAX_DEPTH: usize = 8;

/// The area of each path, in the same order.
///
/// Every path starts in its top-level folder. While one area holds more than
/// half of all the paths, it is split by the next folder down: a `crates/` or
/// `packages/` folder that holds most of the project is not a useful group,
/// but its members are. Areas that are not that large stay as they are.
pub(super) fn assign(paths: &[&str]) -> Vec<String> {
    let folders: Vec<Vec<&str>> = paths.iter().map(|path| directories(path)).collect();
    let mut depth = vec![1_usize; paths.len()];
    loop {
        let mut sizes: HashMap<String, usize> = HashMap::new();
        for (index, path) in folders.iter().enumerate() {
            *sizes.entry(area(path, depth[index])).or_default() += 1;
        }
        let Some((largest, _)) = sizes.into_iter().find(|(_, size)| size * 2 > paths.len()) else {
            break;
        };
        let deeper: Vec<usize> = (0..paths.len())
            .filter(|&index| {
                area(&folders[index], depth[index]) == largest
                    && folders[index].len() > depth[index]
                    && depth[index] < MAX_DEPTH
            })
            .collect();
        if deeper.is_empty() {
            break;
        }
        for index in deeper {
            depth[index] += 1;
        }
    }
    folders
        .iter()
        .zip(&depth)
        .map(|(path, depth)| area(path, *depth))
        .collect()
}

/// The folders of a file path, without the file name.
fn directories(path: &str) -> Vec<&str> {
    let mut parts: Vec<&str> = path.split('/').filter(|part| !part.is_empty()).collect();
    parts.pop();
    parts
}

/// The first `depth` folders joined, or [`ROOT`] when there are none.
fn area(folders: &[&str], depth: usize) -> String {
    let shown = &folders[..depth.min(folders.len())];
    if shown.is_empty() {
        ROOT.to_string()
    } else {
        shown.join("/")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn areas(paths: &[&str]) -> Vec<String> {
        assign(paths)
    }

    #[test]
    fn a_script_in_the_root_has_the_root_area_only() {
        assert_eq!(areas(&["hello.py", "util.py"]), vec![ROOT, ROOT]);
    }

    #[test]
    fn the_top_level_folders_of_the_project_are_the_areas() {
        assert_eq!(
            areas(&["website/a.ts", "app/b.ts", "lib/c.ts", "lib/d.ts"]),
            vec!["website", "app", "lib", "lib"]
        );
    }

    #[test]
    fn a_folder_holding_most_of_the_project_is_split_by_its_children() {
        let paths = [
            "crates/api/a.rs",
            "crates/api/b.rs",
            "crates/db/c.rs",
            "crates/web/d.rs",
            "docs/e.md",
        ];
        assert_eq!(
            areas(&paths),
            vec![
                "crates/api",
                "crates/api",
                "crates/db",
                "crates/web",
                "docs"
            ]
        );
    }

    #[test]
    fn files_directly_in_a_split_folder_stay_in_it() {
        let paths = ["src/main.rs", "src/ui/a.rs", "src/ui/b.rs", "src/db/c.rs"];
        assert_eq!(areas(&paths), vec!["src", "src/ui", "src/ui", "src/db"]);
    }

    #[test]
    fn a_long_common_path_is_walked_until_the_folders_differ() {
        let paths = [
            "app/src/main/java/com/acme/ui/a.kt",
            "app/src/main/java/com/acme/ui/b.kt",
            "app/src/main/java/com/acme/data/c.kt",
            "app/src/test/java/com/acme/d.kt",
        ];
        assert_eq!(
            areas(&paths),
            vec![
                "app/src/main/java/com/acme/ui",
                "app/src/main/java/com/acme/ui",
                "app/src/main/java/com/acme/data",
                "app/src/test"
            ]
        );
    }

    #[test]
    fn two_equal_halves_are_not_split() {
        assert_eq!(
            areas(&["app/x/a.ts", "app/y/b.ts", "web/x/c.ts", "web/y/d.ts"]),
            vec!["app", "app", "web", "web"]
        );
    }

    #[test]
    fn names_stop_after_the_maximum_depth() {
        let paths = ["a/b/c/d/e/f/g/h/i/j.rs", "a/b/c/d/e/f/g/h/i/k.rs"];
        assert_eq!(areas(&paths), vec!["a/b/c/d/e/f/g/h"; 2]);
    }

    #[test]
    fn an_empty_list_has_no_areas() {
        assert!(areas(&[]).is_empty());
    }
}
