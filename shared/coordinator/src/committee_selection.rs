use std::cmp::Ordering;

use psyche_core::{MerkleTree, Proof, sha256v};

pub const COMMITTEE_SALT: &'static str = "committee";
pub const WITNESS_SALT: &'static str = "witness";

#[derive(Debug, PartialEq)]
pub enum Committee {
    TieBreaker,
    Verifier,
    Trainer,
}

pub struct CommitteeAndWitnessWithProof<'a> {
    pub committee: Committee,
    pub committee_position: usize,
    pub committee_proof: Proof<'a>,
    pub witness: bool,
    pub witness_position: usize,
    pub witness_proof: Proof<'a>,
}

#[derive(Eq)]
struct OrderEntry {
    rank: u64,
    index: usize,
}

impl PartialOrd for OrderEntry {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for OrderEntry {
    fn cmp(&self, other: &Self) -> Ordering {
        self.rank.cmp(&other.rank)
    }
}

impl PartialEq for OrderEntry {
    fn eq(&self, other: &Self) -> bool {
        self.rank == other.rank
    }
}

pub fn tree_item(salt: &[u8], seed: u64, id: &[u8], index: usize) -> [u8; 32] {
    sha256v(&[salt, &seed.to_be_bytes(), id, &(index as u64).to_be_bytes()])
}

pub struct CommitteeSelection<'a, T> {
    committee_order: Vec<&'a T>,
    committee_tree: MerkleTree,
    tie_breaker_nodes: usize,
    verifier_nodes: usize,
    witness_order: Vec<&'a T>,
    witness_tree: MerkleTree,
    witness_nodes: usize,
    seed: u64,
}

impl<'a, T> CommitteeSelection<'a, T>
where
    T: AsRef<[u8]> + Eq,
{
    pub fn new(
        tie_breaker_nodes: usize,
        witness_nodes: usize,
        verification_percent: u8,
        nodes: &'a [T],
        seed: u64,
    ) -> Self {
        assert!(nodes.len() < u64::MAX as usize);
        assert!(nodes.len() >= tie_breaker_nodes);
        assert!(nodes.len() >= witness_nodes);
        assert!(verification_percent <= 100);

        let (committee_order, committee_tree) =
            Self::make_order_and_tree(COMMITTEE_SALT, seed, nodes);
        let (witness_order, witness_tree) = Self::make_order_and_tree(WITNESS_SALT, seed, nodes);

        let free_nodes = nodes.len() - tie_breaker_nodes;
        let verifier_nodes = (free_nodes * verification_percent as usize) / 100;

        Self {
            committee_order,
            committee_tree,
            tie_breaker_nodes,
            verifier_nodes,
            witness_order,
            witness_tree,
            witness_nodes,
            seed,
        }
    }

    fn make_order_and_tree(salt: &str, seed: u64, nodes: &'a [T]) -> (Vec<&'a T>, MerkleTree) {
        let mut order_entries: Vec<_> = nodes
            .iter()
            .enumerate()
            .map(|(index, x)| OrderEntry {
                rank: Self::get_rank(salt.as_bytes(), &seed.to_be_bytes(), x.as_ref()),
                index,
            })
            .collect();
        order_entries.sort();
        let tree_items: Vec<_> = order_entries
            .iter()
            .enumerate()
            .map(|(index, item)| {
                tree_item(salt.as_bytes(), seed, nodes[item.index].as_ref(), index)
            })
            .collect();
        let order = order_entries.into_iter().map(|x| &nodes[x.index]).collect();
        let tree = MerkleTree::new(&tree_items);
        (order, tree)
    }

    fn get_rank(salt: &[u8], seed: &[u8], id: &[u8]) -> u64 {
        u64::from_be_bytes(sha256v(&[salt, seed, id])[0..8].try_into().unwrap())
    }

    pub fn get_selection(&self, item: &T) -> CommitteeAndWitnessWithProof {
        let witness_position = self.witness_order.iter().position(|x| *x == item).unwrap();
        let witness_proof = self.witness_tree.find_path(witness_position).unwrap();
        let committee_position = self
            .committee_order
            .iter()
            .position(|x| *x == item)
            .unwrap();
        let committee_proof = self.committee_tree.find_path(committee_position).unwrap();
        let committee = if committee_position < self.tie_breaker_nodes {
            Committee::TieBreaker
        } else if committee_position < self.tie_breaker_nodes + self.verifier_nodes {
            Committee::Verifier
        } else {
            Committee::Trainer
        };
        CommitteeAndWitnessWithProof {
            committee,
            committee_position,
            committee_proof,
            witness: witness_position < self.witness_nodes,
            witness_position,
            witness_proof,
        }
    }

    pub fn get_seed(&self) -> u64 {
        self.seed
    }

    pub fn get_num_tie_breaker_nodes(&self) -> usize {
        self.tie_breaker_nodes
    }

    pub fn get_num_verifier_nodes(&self) -> usize {
        self.verifier_nodes
    }

    pub fn get_num_trainer_nodes(&self) -> usize {
        self.committee_order.len() - self.tie_breaker_nodes - self.verifier_nodes
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_committee_selection_creation() {
        let nodes: Vec<String> = (0..100).map(|i| format!("Node{}", i)).collect();
        let selection = CommitteeSelection::new(10, 20, 30, &nodes, 12345);

        assert_eq!(selection.tie_breaker_nodes, 10);
        assert_eq!(selection.witness_nodes, 20);
        assert_eq!(selection.verifier_nodes, 27); // 30% of (100 - 10) = 27
        assert_eq!(selection.committee_order.len(), 100);
        assert_eq!(selection.witness_order.len(), 100);
    }

    #[test]
    fn test_get_selection() {
        let nodes: Vec<String> = (0..100).map(|i| format!("Node{}", i)).collect();
        let selection: CommitteeSelection<'_, String> =
            CommitteeSelection::new(10, 20, 30, &nodes, 12345);

        for node in &nodes {
            let result = selection.get_selection(&node);
            assert!(matches!(
                result.committee,
                Committee::TieBreaker | Committee::Verifier | Committee::Trainer
            ));
            assert!(result.committee_proof.verify_item(&tree_item(
                COMMITTEE_SALT.as_bytes(),
                selection.get_seed(),
                node.as_ref(),
                result.committee_position
            )));
            assert!(result.witness_proof.verify_item(&tree_item(
                WITNESS_SALT.as_bytes(),
                selection.get_seed(),
                node.as_ref(),
                result.witness_position
            )));
        }
    }

    #[test]
    fn test_deterministic_selection() {
        let nodes: Vec<String> = (0..100).map(|i: i32| format!("Node{}", i)).collect();
        let selection1 = CommitteeSelection::new(10, 20, 30, &nodes, 12345);
        let selection2 = CommitteeSelection::new(10, 20, 30, &nodes, 12345);

        for node in &nodes {
            let result1 = selection1.get_selection(node);
            let result2 = selection2.get_selection(node);
            assert_eq!(result1.committee, result2.committee);
            assert_eq!(result1.witness, result2.witness);
        }
    }

    #[test]
    fn test_different_seeds_produce_different_results() {
        let nodes: Vec<String> = (0..100).map(|i| format!("Node{}", i)).collect();
        let selection1 = CommitteeSelection::new(10, 20, 30, &nodes, 12345);
        let selection2 = CommitteeSelection::new(10, 20, 30, &nodes, 54321);

        let mut all_same = true;
        for node in &nodes {
            let result1 = selection1.get_selection(node);
            let result2 = selection2.get_selection(node);
            if result1.committee != result2.committee || result1.witness != result2.witness {
                all_same = false;
                break;
            }
        }
        assert!(!all_same);
    }
}
