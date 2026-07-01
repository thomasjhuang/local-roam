//! Folgezettel addressing — a pure function from a manifest tree to card addresses.
//!
//! A thread's manifest is a nested list of card links; a card's address is a pure
//! function of its position in that list. Trunk items number `1, 2, 3`; each nesting
//! level alternates letters and numbers (`2`, `2a`, `2a1`, `2a1a`). Addresses are
//! DERIVED, never stored on disk — reorder the manifest and every address is recomputed,
//! always currently-true, never a decaying historical artifact.
//!
//! A card may appear in more than one thread and therefore has more than one address:
//! call [`addresses`] once per thread. Nothing in this module reads or writes files or
//! touches the vault — it is unit-testable in complete isolation.

/// A node in a thread's manifest tree: a card link plus its nested children.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ManifestNode {
    pub card_id: String,
    pub children: Vec<ManifestNode>,
}

impl ManifestNode {
    pub fn new(card_id: impl Into<String>, children: Vec<ManifestNode>) -> Self {
        Self {
            card_id: card_id.into(),
            children,
        }
    }

    /// A childless node.
    pub fn leaf(card_id: impl Into<String>) -> Self {
        Self::new(card_id, Vec::new())
    }
}

/// A derived address for one card in one thread.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Address {
    pub card_id: String,
    /// The Folgezettel address, e.g. `2a1`.
    pub address: String,
    /// Position in a pre-order (document-order) walk of the manifest, from 0.
    pub position: usize,
}

/// Derive an [`Address`] for every card in the tree, in pre-order (document) order.
/// Pure: the same tree always yields the same addresses.
pub fn addresses(tree: &[ManifestNode]) -> Vec<Address> {
    let mut out = Vec::new();
    walk(tree, "", 0, &mut out);
    out
}

fn walk(nodes: &[ManifestNode], prefix: &str, depth: usize, out: &mut Vec<Address>) {
    for (i, node) in nodes.iter().enumerate() {
        let address = format!("{prefix}{}", component(depth, i + 1));
        out.push(Address {
            card_id: node.card_id.clone(),
            address: address.clone(),
            position: out.len(),
        });
        walk(&node.children, &address, depth + 1, out);
    }
}

/// The address component for a node at `depth` (0 = trunk) and 1-based sibling
/// `index`. Even depths number (`1, 2, 3…`); odd depths letter (`a, b, … z, aa, ab…`).
fn component(depth: usize, index: usize) -> String {
    if depth.is_multiple_of(2) {
        index.to_string()
    } else {
        letters(index)
    }
}

/// Bijective base-26 letters: `1→a`, `26→z`, `27→aa`, `28→ab`, … (never empty for
/// `index >= 1`).
fn letters(mut index: usize) -> String {
    let mut chars = Vec::new();
    while index > 0 {
        index -= 1;
        chars.push((b'a' + (index % 26) as u8) as char);
        index /= 26;
    }
    chars.iter().rev().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pairs(tree: &[ManifestNode]) -> Vec<(String, String)> {
        addresses(tree)
            .into_iter()
            .map(|a| (a.card_id, a.address))
            .collect()
    }

    #[test]
    fn trunk_items_number_1_2_3() {
        let tree = vec![
            ManifestNode::leaf("a"),
            ManifestNode::leaf("b"),
            ManifestNode::leaf("c"),
        ];
        assert_eq!(
            pairs(&tree),
            vec![
                ("a".into(), "1".into()),
                ("b".into(), "2".into()),
                ("c".into(), "3".into()),
            ]
        );
    }

    #[test]
    fn nesting_alternates_letters_and_numbers() {
        // The CONTEXT.md example:
        //   - a            1
        //   - b            2
        //     - c          2a
        //       - d        2a1
        //     - e          2b
        //   - f            3
        let tree = vec![
            ManifestNode::leaf("a"),
            ManifestNode::new(
                "b",
                vec![
                    ManifestNode::new("c", vec![ManifestNode::leaf("d")]),
                    ManifestNode::leaf("e"),
                ],
            ),
            ManifestNode::leaf("f"),
        ];
        assert_eq!(
            pairs(&tree),
            vec![
                ("a".into(), "1".into()),
                ("b".into(), "2".into()),
                ("c".into(), "2a".into()),
                ("d".into(), "2a1".into()),
                ("e".into(), "2b".into()),
                ("f".into(), "3".into()),
            ]
        );
    }

    #[test]
    fn a_fourth_level_alternates_back_to_letters() {
        // 2 → 2a → 2a1 → 2a1a
        let tree = vec![ManifestNode::leaf("x1"), ManifestNode::new(
            "two",
            vec![ManifestNode::new(
                "twoa",
                vec![ManifestNode::new("twoa1", vec![ManifestNode::leaf("twoa1a")])],
            )],
        )];
        let got = pairs(&tree);
        assert_eq!(got.last().unwrap(), &("twoa1a".to_string(), "2a1a".to_string()));
    }

    #[test]
    fn letters_wrap_past_z_bijectively() {
        assert_eq!(letters(1), "a");
        assert_eq!(letters(26), "z");
        assert_eq!(letters(27), "aa");
        assert_eq!(letters(28), "ab");
        assert_eq!(letters(52), "az");
        assert_eq!(letters(53), "ba");
        // A trunk node with 27 children: the 27th branch is 1aa.
        let children: Vec<ManifestNode> = (0..27).map(|i| ManifestNode::leaf(format!("c{i}"))).collect();
        let tree = vec![ManifestNode::new("root", children)];
        let got = pairs(&tree);
        assert_eq!(got[27].1, "1aa", "the 27th child branches to 1aa");
    }

    #[test]
    fn a_card_in_two_threads_gets_two_addresses() {
        // The same card sits at different positions in two different manifests, so it
        // has a different derived address in each — the many-to-many Luhmann's paper
        // filing could not express.
        let thread_one = vec![
            ManifestNode::leaf("other"),
            ManifestNode::new("shared", vec![ManifestNode::leaf("tail")]),
        ];
        let thread_two = vec![ManifestNode::leaf("shared")];

        let addr_one = addresses(&thread_one)
            .into_iter()
            .find(|a| a.card_id == "shared")
            .unwrap()
            .address;
        let addr_two = addresses(&thread_two)
            .into_iter()
            .find(|a| a.card_id == "shared")
            .unwrap()
            .address;

        assert_eq!(addr_one, "2");
        assert_eq!(addr_two, "1");
        assert_ne!(addr_one, addr_two, "one card, two threads, two addresses");
    }

    #[test]
    fn empty_tree_yields_no_addresses() {
        assert!(addresses(&[]).is_empty());
    }
}
