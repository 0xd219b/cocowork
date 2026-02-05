//! Topic tree data structures

/// Node in the topic tree (left sidebar)
#[derive(Debug, Clone)]
pub struct TopicNode {
    /// Unique identifier
    pub id: String,
    /// Display name
    pub name: String,
    /// Optional icon name
    pub icon: Option<String>,
    /// Whether this node is expanded (for folders)
    pub is_expanded: bool,
    /// Child nodes
    pub children: Vec<TopicNode>,
}

impl TopicNode {
    /// Create a new folder node
    pub fn folder(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            icon: Some("folder".to_string()),
            is_expanded: false,
            children: Vec::new(),
        }
    }

    /// Create a new leaf node (no children)
    pub fn leaf(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            icon: None,
            is_expanded: false,
            children: Vec::new(),
        }
    }

    /// Check if this node has children
    pub fn has_children(&self) -> bool {
        !self.children.is_empty()
    }

    /// Toggle expanded state
    pub fn toggle_expanded(&mut self) {
        if self.has_children() {
            self.is_expanded = !self.is_expanded;
        }
    }

    /// Add a child node
    pub fn add_child(&mut self, child: TopicNode) {
        self.children.push(child);
        self.icon = Some("folder".to_string());
    }

    /// Find a node by ID recursively
    pub fn find_by_id(&self, id: &str) -> Option<&TopicNode> {
        if self.id == id {
            return Some(self);
        }
        for child in &self.children {
            if let Some(found) = child.find_by_id(id) {
                return Some(found);
            }
        }
        None
    }

    /// Find a node by ID mutably
    pub fn find_by_id_mut(&mut self, id: &str) -> Option<&mut TopicNode> {
        if self.id == id {
            return Some(self);
        }
        for child in &mut self.children {
            if let Some(found) = child.find_by_id_mut(id) {
                return Some(found);
            }
        }
        None
    }

    /// Get the depth of this node in the tree
    pub fn depth(&self) -> usize {
        if self.children.is_empty() {
            0
        } else {
            1 + self.children.iter().map(|c| c.depth()).max().unwrap_or(0)
        }
    }

    /// Flatten the tree for rendering (with depth info)
    pub fn flatten(&self, depth: usize) -> Vec<(usize, &TopicNode)> {
        let mut result = vec![(depth, self)];
        if self.is_expanded {
            for child in &self.children {
                result.extend(child.flatten(depth + 1));
            }
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_topic_node_creation() {
        let folder = TopicNode::folder("test-folder", "Test Folder");
        assert_eq!(folder.id, "test-folder");
        assert_eq!(folder.name, "Test Folder");
        assert!(folder.icon.is_some());
        assert!(!folder.has_children());

        let leaf = TopicNode::leaf("test-leaf", "Test Leaf");
        assert!(leaf.icon.is_none());
    }

    #[test]
    fn test_topic_node_children() {
        let mut parent = TopicNode::folder("parent", "Parent");
        parent.add_child(TopicNode::leaf("child1", "Child 1"));
        parent.add_child(TopicNode::leaf("child2", "Child 2"));

        assert!(parent.has_children());
        assert_eq!(parent.children.len(), 2);
    }

    #[test]
    fn test_topic_node_find() {
        let mut root = TopicNode::folder("root", "Root");
        let mut child = TopicNode::folder("child", "Child");
        child.add_child(TopicNode::leaf("grandchild", "Grandchild"));
        root.add_child(child);

        assert!(root.find_by_id("root").is_some());
        assert!(root.find_by_id("child").is_some());
        assert!(root.find_by_id("grandchild").is_some());
        assert!(root.find_by_id("nonexistent").is_none());
    }

    #[test]
    fn test_topic_node_flatten() {
        let mut root = TopicNode::folder("root", "Root");
        root.is_expanded = true;
        root.add_child(TopicNode::leaf("child1", "Child 1"));
        root.add_child(TopicNode::leaf("child2", "Child 2"));

        let flattened = root.flatten(0);
        assert_eq!(flattened.len(), 3);
        assert_eq!(flattened[0].0, 0); // root at depth 0
        assert_eq!(flattened[1].0, 1); // child1 at depth 1
        assert_eq!(flattened[2].0, 1); // child2 at depth 1
    }
}
