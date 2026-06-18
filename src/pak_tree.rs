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
    let mut items = Vec::new();

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

    let mut all_paths = std::collections::HashSet::new();
    for dir in &dirs_set {
        all_paths.insert((dir.clone(), true));
    }
    for file in file_paths {
        all_paths.insert((file.clone(), false));
    }

    let mut sorted_paths: Vec<(String, bool)> = all_paths.into_iter().collect();
    sorted_paths.sort_by(|a, b| a.0.cmp(&b.0));

    for (path, is_dir) in sorted_paths {
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

fn is_visible(item: &TreeItem, items: &[TreeItem]) -> bool {
    let parts: Vec<&str> = item.path.split('\\').filter(|s| !s.is_empty()).collect();
    if parts.len() <= 1 {
        return true;
    }

    let mut current = String::new();
    for part in parts.iter().take(parts.len().saturating_sub(1)) {
        if !current.is_empty() {
            current.push('\\');
        }
        current.push_str(part);

        let parent_collapsed = items
            .iter()
            .any(|it| it.is_dir && it.path == current && !it.expanded);
        if parent_collapsed {
            return false;
        }
    }
    true
}

pub fn get_visible_tree_nodes(items: &[TreeItem]) -> Vec<String> {
    let mut out = Vec::new();
    for item in items {
        if is_visible(item, items) {
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
        }
    }
    out
}

pub fn toggle_tree_node(items: &mut [TreeItem], visible_index: usize) -> bool {
    let mut visible_indices = Vec::new();
    for (idx, item) in items.iter().enumerate() {
        if is_visible(item, items) {
            visible_indices.push(idx);
        }
    }

    if let Some(&full_idx) = visible_indices
        .get(visible_index)
        .filter(|&&idx| items[idx].is_dir)
    {
        items[full_idx].expanded = !items[full_idx].expanded;
        true
    } else {
        false
    }
}
