use crate::types::CleanupPlan;
use std::{io, path::PathBuf};
use thiserror::Error;
use tokio::fs;

/// Chyby bezpečného odstránenia dočasného workspace.
#[derive(Debug, Error)]
pub enum CleanupError {
    #[error("nepodarilo sa overiť cleanup root `{path}`: {source}")]
    CanonicalizeRoot {
        path: PathBuf,
        #[source]
        source: io::Error,
    },

    #[error("nepodarilo sa overiť workspace `{path}`: {source}")]
    CanonicalizeWorkspace {
        path: PathBuf,
        #[source]
        source: io::Error,
    },

    #[error("odmietnuté nebezpečné cleanup cesty: workspace `{workspace}` nie je potomkom root `{root}`")]
    UnsafePath { root: PathBuf, workspace: PathBuf },

    #[error("nepodarilo sa odstrániť dočasný workspace `{path}`: {source}")]
    Remove {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
}

/// Bezpečne odstráni workspace iba v rámci vopred povoleného rootu.
///
/// Pred odstránením sa obe cesty canonicalizujú. Symlink smerujúci mimo
/// povolený temp root je odmietnutý, rovnako ako pokus odstrániť samotný root.
pub async fn cleanup_workspace(plan: &CleanupPlan, successful: bool) -> Result<(), CleanupError> {
    if !plan.should_cleanup(successful) {
        return Ok(());
    }

    let canonical_root = match fs::canonicalize(&plan.allowed_root).await {
        Ok(path) => path,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(source) => {
            return Err(CleanupError::CanonicalizeRoot {
                path: plan.allowed_root.clone(),
                source,
            });
        }
    };

    let canonical_workspace = match fs::canonicalize(&plan.workspace).await {
        Ok(path) => path,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(source) => {
            return Err(CleanupError::CanonicalizeWorkspace {
                path: plan.workspace.clone(),
                source,
            });
        }
    };

    if canonical_workspace == canonical_root || !canonical_workspace.starts_with(&canonical_root) {
        return Err(CleanupError::UnsafePath {
            root: canonical_root,
            workspace: canonical_workspace,
        });
    }

    fs::remove_dir_all(&canonical_workspace)
        .await
        .map_err(|source| CleanupError::Remove {
            path: canonical_workspace,
            source,
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn test_root() -> PathBuf {
        std::env::temp_dir().join(format!(
            "dabovanie-cleanup-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system clock before Unix epoch")
                .as_nanos()
        ))
    }

    #[tokio::test]
    async fn removes_only_workspace_below_root() {
        let root = test_root();
        let workspace = root.join("job");
        fs::create_dir_all(&workspace).await.unwrap();
        fs::write(workspace.join("intermediate.bin"), b"test")
            .await
            .unwrap();

        cleanup_workspace(
            &CleanupPlan {
                allowed_root: root.clone(),
                workspace: workspace.clone(),
                on_success: true,
                on_failure: true,
            },
            true,
        )
        .await
        .unwrap();

        assert!(!workspace.exists());
        assert!(root.exists());
        fs::remove_dir_all(root).await.unwrap();
    }

    #[tokio::test]
    async fn refuses_to_remove_root_itself() {
        let root = test_root();
        fs::create_dir_all(&root).await.unwrap();

        let error = cleanup_workspace(
            &CleanupPlan {
                allowed_root: root.clone(),
                workspace: root.clone(),
                on_success: true,
                on_failure: true,
            },
            true,
        )
        .await
        .unwrap_err();

        assert!(matches!(error, CleanupError::UnsafePath { .. }));
        fs::remove_dir_all(root).await.unwrap();
    }
}
