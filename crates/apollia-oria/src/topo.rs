//! Tri topologique des steps d'un `ExecutionPlan` basé sur leurs dépendances.
//!
//! Ce module expose [`topological_sort`], une fonction pure utilisée par deux
//! consommateurs :
//! - [`crate::reasoner::Reasoner::parse_and_validate`] — pour détecter les cycles
//!   avant d'accepter un plan généré par le LLM.
//! - `ActorLoop` — pour obtenir l'ordre d'exécution séquentiel des steps.

use std::collections::{HashMap, VecDeque};

use crate::plan::PlanStep;

/// Erreur retournée si un cycle est détecté dans les dépendances du plan.
///
/// Indique qu'au moins deux steps forment une dépendance circulaire, rendant
/// l'exécution ordonnée impossible.
#[derive(Debug, thiserror::Error)]
#[error("Circular dependency detected in execution plan")]
pub struct CycleError;

/// Groups steps into execution levels.
///
/// Steps in the same level share no dependency edge between them and can therefore
/// be executed concurrently. Steps in level N must all complete before any step in
/// level N+1 starts.
///
/// Returns `Err(CycleError)` if the dependency graph contains a cycle.
///
/// **Algorithm**: standard BFS level-assignment after in-degree initialisation.
/// All steps with in-degree 0 form level 0. After processing a level, each step
/// whose in-degree reaches 0 is placed in the next level.
pub fn topological_levels(steps: &[PlanStep]) -> Result<Vec<Vec<String>>, CycleError> {
    if steps.is_empty() {
        return Ok(vec![]);
    }

    let id_to_idx: HashMap<&str, usize> = steps
        .iter()
        .enumerate()
        .map(|(i, s)| (s.step_id.as_str(), i))
        .collect();

    let mut in_degree = vec![0usize; steps.len()];
    for (i, step) in steps.iter().enumerate() {
        for dep in &step.depends_on {
            if id_to_idx.contains_key(dep.as_str()) {
                in_degree[i] += 1;
            }
        }
    }

    let mut dependents: HashMap<&str, Vec<usize>> = HashMap::new();
    for (i, step) in steps.iter().enumerate() {
        for dep in &step.depends_on {
            dependents.entry(dep.as_str()).or_default().push(i);
        }
    }

    let mut current_level: Vec<usize> = in_degree
        .iter()
        .enumerate()
        .filter(|(_, &d)| d == 0)
        .map(|(i, _)| i)
        .collect();

    let mut levels: Vec<Vec<String>> = Vec::new();
    let mut processed = 0usize;

    while !current_level.is_empty() {
        let level_ids: Vec<String> = current_level
            .iter()
            .map(|&i| steps[i].step_id.clone())
            .collect();
        processed += current_level.len();

        // Build the next level: steps unblocked by completing the current level.
        let mut next_level: Vec<usize> = Vec::new();
        for &idx in &current_level {
            if let Some(dep_indices) = dependents.get(steps[idx].step_id.as_str()) {
                for &dep_idx in dep_indices {
                    in_degree[dep_idx] -= 1;
                    if in_degree[dep_idx] == 0 {
                        next_level.push(dep_idx);
                    }
                }
            }
        }

        levels.push(level_ids);
        current_level = next_level;
    }

    if processed != steps.len() {
        Err(CycleError)
    } else {
        Ok(levels)
    }
}

/// Tri topologique des steps d'un `ExecutionPlan` basé sur leurs dépendances.
///
/// Retourne la liste des `step_id` dans un ordre d'exécution valide : chaque step
/// apparaît après tous les steps listés dans son `depends_on`.
/// Retourne `Err(CycleError)` si les dépendances forment un cycle.
///
/// **Algorithme de Kahn (BFS) :**
/// 1. Calculer le in-degree de chaque step (nombre de dépendances valides directes)
/// 2. Initialiser la queue avec les steps sans dépendances (in-degree = 0)
/// 3. Tant que la queue est non vide :
///    a. Prendre un step de la queue → l'ajouter au résultat
///    b. Pour chaque step qui en dépend : décrémenter son in-degree
///    c. Si in-degree atteint 0 → l'ajouter à la queue
/// 4. Si `résultat.len() != steps.len()` → cycle détecté
///
/// La fonction est **pure** — pas d'état mutable global, même entrée = même sortie.
/// Pas de récursion — évite les stack overflows sur de grands plans.
pub fn topological_sort(steps: &[PlanStep]) -> Result<Vec<String>, CycleError> {
    if steps.is_empty() {
        return Ok(vec![]);
    }

    // Build step_id → Vec index map
    let id_to_idx: HashMap<&str, usize> = steps
        .iter()
        .enumerate()
        .map(|(i, s)| (s.step_id.as_str(), i))
        .collect();

    // in_degree[i] = number of valid dependencies step i has (predecessors)
    let mut in_degree = vec![0usize; steps.len()];
    for (i, step) in steps.iter().enumerate() {
        for dep in &step.depends_on {
            if id_to_idx.contains_key(dep.as_str()) {
                in_degree[i] += 1;
            }
            // Unknown deps are ignored — already validated by parse_and_validate()
        }
    }

    // Adjacency list: when step X completes, which step indices can be unblocked?
    // dependents[step_id] = list of indices that depend on step_id
    let mut dependents: HashMap<&str, Vec<usize>> = HashMap::new();
    for (i, step) in steps.iter().enumerate() {
        for dep in &step.depends_on {
            dependents.entry(dep.as_str()).or_default().push(i);
        }
    }

    // Initialize queue with steps that have no dependencies
    let mut queue: VecDeque<usize> = in_degree
        .iter()
        .enumerate()
        .filter(|(_, &d)| d == 0)
        .map(|(i, _)| i)
        .collect();

    let mut result = Vec::with_capacity(steps.len());

    while let Some(idx) = queue.pop_front() {
        result.push(steps[idx].step_id.clone());

        // For each step that depends on the current step, decrement its in-degree
        if let Some(dependent_indices) = dependents.get(steps[idx].step_id.as_str()) {
            for &dep_idx in dependent_indices {
                in_degree[dep_idx] -= 1;
                if in_degree[dep_idx] == 0 {
                    queue.push_back(dep_idx);
                }
            }
        }
    }

    if result.len() != steps.len() {
        Err(CycleError)
    } else {
        Ok(result)
    }
}

// ─────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn step(id: &str, deps: &[&str]) -> PlanStep {
        PlanStep {
            step_id: id.to_string(),
            description: format!("Step {id}"),
            tool_hint: None,
            depends_on: deps.iter().map(|s| s.to_string()).collect(),
            model_hint: None,
        }
    }

    /// GIVEN [s1 (no deps), s2 (depends_on: [s1]), s3 (depends_on: [s2])]
    /// WHEN topological_sort(&steps) est appelé
    /// THEN Ok(["s1", "s2", "s3"]) est retourné
    #[test]
    fn test_ac1_plan_lineaire() {
        // GIVEN
        let steps = vec![step("s1", &[]), step("s2", &["s1"]), step("s3", &["s2"])];
        // WHEN
        let result = topological_sort(&steps).unwrap();
        // THEN
        assert_eq!(result, vec!["s1", "s2", "s3"]);
    }

    /// GIVEN [s1, s2, s3 (tous sans deps), s4 (depends_on: [s1, s2, s3])]
    /// WHEN topological_sort(&steps) est appelé
    /// THEN s1, s2, s3 apparaissent avant s4
    #[test]
    fn test_ac2_branches_paralleles() {
        // GIVEN
        let steps = vec![
            step("s1", &[]),
            step("s2", &[]),
            step("s3", &[]),
            step("s4", &["s1", "s2", "s3"]),
        ];
        // WHEN
        let result = topological_sort(&steps).unwrap();
        // THEN
        assert_eq!(result.len(), 4);
        let s4_pos = result.iter().position(|s| s == "s4").unwrap();
        for s in ["s1", "s2", "s3"] {
            let pos = result.iter().position(|x| x == s).unwrap();
            assert!(pos < s4_pos, "{s} doit précéder s4");
        }
    }

    /// GIVEN [s1 (depends_on: [s2]), s2 (depends_on: [s1])]
    /// WHEN topological_sort(&steps) est appelé
    /// THEN Err(CycleError) est retourné
    #[test]
    fn test_ac3_cycle_simple() {
        // GIVEN
        let steps = vec![step("s1", &["s2"]), step("s2", &["s1"])];
        // WHEN
        let result = topological_sort(&steps);
        // THEN
        assert!(matches!(result, Err(CycleError)));
    }

    /// GIVEN [s1→s3, s2→s1, s3→s2] (cycle long)
    /// WHEN topological_sort(&steps) est appelé
    /// THEN Err(CycleError) est retourné
    #[test]
    fn test_ac4_cycle_long() {
        // GIVEN
        let steps = vec![
            step("s1", &["s3"]),
            step("s2", &["s1"]),
            step("s3", &["s2"]),
        ];
        // WHEN
        let result = topological_sort(&steps);
        // THEN
        assert!(matches!(result, Err(CycleError)));
    }

    /// GIVEN liste vide
    /// WHEN topological_sort(&[]) est appelé
    /// THEN Ok([]) est retourné
    #[test]
    fn test_ac5_plan_vide() {
        // GIVEN / WHEN / THEN
        let result = topological_sort(&[]).unwrap();
        assert!(result.is_empty());
    }

    /// GIVEN [s2 (depends_on: [s1]), s1 (no deps)] — ordre inversé dans le Vec
    /// WHEN topological_sort(&steps) est appelé
    /// THEN s1 apparaît avant s2 dans le résultat
    #[test]
    fn test_ac6_ordre_entree_irrelevant() {
        // GIVEN
        let steps = vec![step("s2", &["s1"]), step("s1", &[])];
        // WHEN
        let result = topological_sort(&steps).unwrap();
        // THEN
        let s1_pos = result.iter().position(|s| s == "s1").unwrap();
        let s2_pos = result.iter().position(|s| s == "s2").unwrap();
        assert!(s1_pos < s2_pos);
    }

    /// GIVEN step unique sans dépendances
    /// WHEN topological_sort est appelé
    /// THEN Ok(["s1"]) est retourné
    #[test]
    fn test_step_unique_sans_dep() {
        // GIVEN / WHEN
        let steps = vec![step("s1", &[])];
        let result = topological_sort(&steps).unwrap();
        // THEN
        assert_eq!(result, vec!["s1"]);
    }

    // -----------------------------------------------------------------------
    // topological_levels tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_levels_empty() {
        // GIVEN / WHEN / THEN
        assert_eq!(topological_levels(&[]).unwrap(), Vec::<Vec<String>>::new());
    }

    #[test]
    fn test_levels_single_step() {
        // GIVEN
        let steps = vec![step("s1", &[])];
        // WHEN
        let levels = topological_levels(&steps).unwrap();
        // THEN
        assert_eq!(levels, vec![vec!["s1".to_string()]]);
    }

    #[test]
    fn test_levels_linear_chain() {
        // GIVEN — s1 → s2 → s3
        let steps = vec![step("s1", &[]), step("s2", &["s1"]), step("s3", &["s2"])];
        // WHEN
        let levels = topological_levels(&steps).unwrap();
        // THEN — three separate levels (no parallelism possible)
        assert_eq!(levels.len(), 3);
        assert_eq!(levels[0], vec!["s1"]);
        assert_eq!(levels[1], vec!["s2"]);
        assert_eq!(levels[2], vec!["s3"]);
    }

    #[test]
    fn test_levels_parallel_fan_out() {
        // GIVEN — s1 (no deps), s2 (no deps), s3 (no deps), s4 (depends on all)
        let steps = vec![
            step("s1", &[]),
            step("s2", &[]),
            step("s3", &[]),
            step("s4", &["s1", "s2", "s3"]),
        ];
        // WHEN
        let levels = topological_levels(&steps).unwrap();
        // THEN — level 0: {s1, s2, s3}; level 1: {s4}
        assert_eq!(levels.len(), 2);
        let mut level0 = levels[0].clone();
        level0.sort();
        assert_eq!(level0, vec!["s1", "s2", "s3"]);
        assert_eq!(levels[1], vec!["s4"]);
    }

    #[test]
    fn test_levels_diamond() {
        // GIVEN — s1 → s2, s1 → s3, s2 → s4, s3 → s4
        let steps = vec![
            step("s1", &[]),
            step("s2", &["s1"]),
            step("s3", &["s1"]),
            step("s4", &["s2", "s3"]),
        ];
        // WHEN
        let levels = topological_levels(&steps).unwrap();
        // THEN — level 0: {s1}, level 1: {s2, s3}, level 2: {s4}
        assert_eq!(levels.len(), 3);
        assert_eq!(levels[0], vec!["s1"]);
        let mut level1 = levels[1].clone();
        level1.sort();
        assert_eq!(level1, vec!["s2", "s3"]);
        assert_eq!(levels[2], vec!["s4"]);
    }

    #[test]
    fn test_levels_cycle_returns_error() {
        // GIVEN
        let steps = vec![step("s1", &["s2"]), step("s2", &["s1"])];
        // WHEN / THEN
        assert!(matches!(topological_levels(&steps), Err(CycleError)));
    }

    /// GIVEN diamant s1→s2, s1→s3, s2→s4, s3→s4
    /// WHEN topological_sort est appelé
    /// THEN s1 avant s4, résultat de longueur 4
    #[test]
    fn test_diamant() {
        // GIVEN
        let steps = vec![
            step("s1", &[]),
            step("s2", &["s1"]),
            step("s3", &["s1"]),
            step("s4", &["s2", "s3"]),
        ];
        // WHEN
        let result = topological_sort(&steps).unwrap();
        // THEN
        assert_eq!(result.len(), 4);
        let s1_pos = result.iter().position(|s| s == "s1").unwrap();
        let s4_pos = result.iter().position(|s| s == "s4").unwrap();
        assert!(s1_pos < s4_pos);
    }
}
