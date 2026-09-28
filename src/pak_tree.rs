#[derive(Clone, Debug)]
pub struct TreeItem {
    pub path: String,
    pub name: String,
    pub is_dir: bool,
    pub indent: usize,
    pub expanded: bool,
}

pub fn generate_tree_items(file_paths: &[String]) -> Vec<TreeItem> {
    let mut dirs_set = std::collections::HashSet::new();

    for path in file_paths {
        let parts: Vec<&str> = path.split('\\').filter(|s| !s.is_empty()).collect();
        let mut current = String::new();
        for part in parts.iter().take(parts.len().saturating_sub(1)) {
            if !current.is_empty() {
                current.push('\\');
            }
            current.push_str(part);
            dirs_set.insert(current.clone());
        }
    }

    let mut all_paths = Vec::with_capacity(dirs_set.len() + file_paths.len());
    for dir in dirs_set {
        all_paths.push((dir, true));
    }
    for file in file_paths {
        all_paths.push((file.clone(), false));
    }

    all_paths.sort_by(|a, b| a.0.cmp(&b.0));

    let mut items = Vec::with_capacity(all_paths.len());
    for (path, is_dir) in all_paths {
        let parts: Vec<&str> = path.split('\\').filter(|s| !s.is_empty()).collect();
        let name = parts.last().cloned().unwrap_or_default().to_string();
        let indent = parts.len().saturating_sub(1);

        items.push(TreeItem {
            path,
            name,
            is_dir,
            indent,
            expanded: true,
        });
    }
    items
}

pub fn get_visible_tree_nodes(items: &[TreeItem]) -> Vec<String> {
    let mut out = Vec::new();
    let mut collapsed_prefix: Option<String> = None;

    for item in items {
        if let Some(ref prefix) = collapsed_prefix {
            if item.path.starts_with(prefix) {
                continue;
            } else {
                collapsed_prefix = None;
            }
        }

        let prefix = "  ".repeat(item.indent);
        let state_icon = if item.is_dir {
            if item.expanded {
                "▼ 📁 "
            } else {
                "▶ 📁 "
            }
        } else {
            "  📄 "
        };
        out.push(format!("{}{}{}", prefix, state_icon, item.name));

        if item.is_dir && !item.expanded {
            collapsed_prefix = Some(format!("{}\\", item.path));
        }
    }
    out
}

pub fn toggle_tree_node(items: &mut [TreeItem], visible_index: usize) -> bool {
    let mut current_visible = 0;
    let mut collapsed_prefix: Option<String> = None;

    for item in items.iter_mut() {
        if let Some(ref prefix) = collapsed_prefix {
            if item.path.starts_with(prefix) {
                continue;
            } else {
                collapsed_prefix = None;
            }
        }

        if current_visible == visible_index {
            if item.is_dir {
                item.expanded = !item.expanded;
                return true;
            }
            return false;
        }

        if item.is_dir && !item.expanded {
            collapsed_prefix = Some(format!("{}\\", item.path));
        }

        current_visible += 1;
    }
    false
}
