use super::custom_folder::CustomFolder;
use itertools::Itertools;
use std::{cell::RefCell, collections::BTreeMap, rc::Rc};

pub trait Hierarchy: Send + Sized + Clone {
    fn local_id(&self) -> u64;
    fn parent_id(&self) -> Option<u64>;
    fn set_children(&mut self, children: Vec<Self>);
    fn display_order(&self) -> u32;
}

impl Hierarchy for CustomFolder {
    fn local_id(&self) -> u64 {
        self.local_id.as_u64()
    }

    fn parent_id(&self) -> Option<u64> {
        self.parent_id.map(|id| id.as_u64())
    }

    fn set_children(&mut self, children: Vec<Self>) {
        self.children = children;
    }

    fn display_order(&self) -> u32 {
        self.display_order
    }
}

pub fn custom_folder_hierarchy<H: Hierarchy>(labels: &[H]) -> Vec<H> {
    let mut index: BTreeMap<_, _> = labels
        .iter()
        .map(|l| {
            (
                l.local_id(),
                Rc::new(RefCell::new(HierarchyMap {
                    id: l.local_id(),
                    parent_id: l.parent_id(),
                    children: vec![],
                })),
            )
        })
        .collect();

    labels
        .iter()
        .filter_map(|label| {
            let parent_id = label.parent_id()?;
            let local_id = label.local_id();

            index.get(&local_id).map(|rc| (parent_id, rc.clone()))
        })
        .collect_vec()
        .into_iter()
        .for_each(|(parent_id, rc)| {
            // This is the core of the algorithm - it relies on the fact that the
            // smart reference Rc<RefCell<HierarchyMap>> mutates all the references
            // at once. This way, we can modify the children of the parent node
            // accessing it at the top level of the map, and propagate the changes
            // through the Rc<RefCell<HierarchyMap>> references.
            index
                .entry(parent_id)
                .and_modify(|f| f.borrow_mut().children.push(rc));
        });

    let mut folder_map: BTreeMap<u64, H> =
        labels.iter().map(|l| (l.local_id(), l.clone())).collect();

    // Map CustomFolders to their hierarchy
    let mut result: Vec<_> = index
        .iter()
        // Keep only root Folders
        .filter(|(_, f)| f.borrow().parent_id.is_none())
        .map(|(_, f)| {
            let mut folder = folder_map.remove(&f.borrow().id).unwrap();
            folder.set_children(f.borrow().map_children(&mut folder_map));
            folder
        })
        .collect();

    result.sort_by_cached_key(|a| a.display_order());

    result
}

#[derive(Debug)]
struct HierarchyMap {
    pub id: u64,
    pub parent_id: Option<u64>,
    pub children: Vec<Rc<RefCell<HierarchyMap>>>,
}

impl HierarchyMap {
    // Map the children of the current node to a vec of CustomFolder
    // Called recursively on all children
    // Note: Sort the children using display_order
    fn map_children<H: Hierarchy>(&self, index: &mut BTreeMap<u64, H>) -> Vec<H> {
        if self.children.is_empty() {
            vec![]
        } else {
            let mut result: Vec<_> = self
                .children
                .iter()
                .map(|c| {
                    let mut folder = index.remove(&c.borrow().id).unwrap();
                    folder.set_children(c.borrow().map_children(index));
                    folder
                })
                .collect();
            result.sort_by_cached_key(|a| a.display_order());
            result
        }
    }
}
