//! Where the MLX engine is, and the command line that starts it.
//!
//! `vllm-mlx` is a Python package. It runs on the interpreter the application
//! already embeds, with its own package directory on `PYTHONPATH` and `-S`, so
//! the agents' packages never mix with the engine's (see
//! `packaging/build-mlx-engine.sh` for the isolation contract).
//!
//! The package directory is found, in order: the `APOLLIA_MLX_ENGINE_DIR`
//! override, the newest MLX environment of the engine store, then the one the
//! application bundles under `python/engines/mlx`. The interpreter is the
//! `APOLLIA_MLX_PYTHON` override, else the bundled one.

use std::path::{Path, PathBuf};

use crate::engine_store::{newest_installed, Engine, Variant};

/// Override of the directory holding the engine's `site-packages`.
pub const ENV_ENGINE_DIR: &str = "APOLLIA_MLX_ENGINE_DIR";

/// Override of the Python interpreter the engine runs on.
pub const ENV_PYTHON: &str = "APOLLIA_MLX_PYTHON";

/// The interpreter of the embedded Python, relative to its root.
const BUNDLED_INTERPRETER: &str = "bin/python3.13";

/// How to start one engine process: the program, the arguments that come
/// before the model, and the environment it needs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MlxLaunch {
    /// Executable to run.
    pub program: PathBuf,
    /// Arguments placed before the model and the listening address.
    pub prefix_args: Vec<String>,
    /// Variables set on the child, after the bundled Python's are scrubbed.
    pub env: Vec<(String, String)>,
}

impl MlxLaunch {
    /// `vllm-mlx serve` on `python`, importing from `site_packages` only.
    #[must_use]
    pub fn vllm_mlx(python: PathBuf, site_packages: &Path) -> Self {
        Self {
            program: python,
            prefix_args: ["-S", "-m", "vllm_mlx.cli", "serve"]
                .into_iter()
                .map(str::to_owned)
                .collect(),
            env: vec![("PYTHONPATH".to_owned(), site_packages.display().to_string())],
        }
    }

    /// The package directory the engine imports from.
    #[must_use]
    pub fn site_packages(&self) -> Option<&Path> {
        self.env
            .iter()
            .find(|(name, _)| name == "PYTHONPATH")
            .map(|(_, value)| Path::new(value.as_str()))
    }

    /// The full argument vector for serving `model_path` on loopback `port`.
    ///
    /// The engine listens on `127.0.0.1` only. Tool calls are parsed by the
    /// engine's automatic parser, which reads the formats of the common chat
    /// templates. No embeddings route is asked for: the engine ships without
    /// the package it would need.
    #[must_use]
    pub fn args(&self, model_path: &str, port: u16) -> Vec<String> {
        let mut args = self.prefix_args.clone();
        args.extend(
            [
                model_path,
                "--host",
                "127.0.0.1",
                "--port",
                &port.to_string(),
                "--enable-auto-tool-choice",
                "--tool-call-parser",
                "auto",
            ]
            .into_iter()
            .map(str::to_owned),
        );
        args
    }
}

/// The engine of this installation, or `None` when none is present.
///
/// `exe_dir` is the directory of the running executable, `store_root` the
/// engine store's, and `env` reads the overrides.
pub fn locate(
    exe_dir: Option<&Path>,
    store_root: Option<&Path>,
    env: impl Fn(&str) -> Option<String>,
) -> Option<MlxLaunch> {
    let bundled_python_roots: Vec<PathBuf> = exe_dir
        .map(|dir| apollia_core::paths::bundled_resource_dirs(dir, "python"))
        .unwrap_or_default();

    let site_packages = env(ENV_ENGINE_DIR)
        .map(PathBuf::from)
        .map(|dir| dir.join("site-packages"))
        .filter(|dir| dir.is_dir())
        .or_else(|| {
            let package = newest_installed(store_root?, Engine::VllmMlx, Variant::Metal)?;
            Some(package.path.join("site-packages")).filter(|dir| dir.is_dir())
        })
        .or_else(|| {
            bundled_python_roots
                .iter()
                .map(|root| root.join("engines").join("mlx").join("site-packages"))
                .find(|dir| dir.is_dir())
        })?;

    let python = env(ENV_PYTHON)
        .map(PathBuf::from)
        .filter(|p| p.is_file())
        .or_else(|| {
            bundled_python_roots
                .iter()
                .map(|root| root.join(BUNDLED_INTERPRETER))
                .find(|p| p.is_file())
        })?;

    tracing::event!(
        tracing::Level::INFO,
        python = %python.display(),
        site_packages = %site_packages.display(),
        "mlx.engine.located"
    );
    Some(MlxLaunch::vllm_mlx(python, &site_packages))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_command_line_serves_the_model_on_loopback_from_the_engine_packages() {
        // GIVEN the engine's interpreter and package directory
        let launch = MlxLaunch::vllm_mlx(
            PathBuf::from("/app/python/bin/python3.13"),
            Path::new("/app/python/engines/mlx/site-packages"),
        );

        // WHEN the command line for a model is built
        let args = launch.args("/models/qwen-mlx", 41234);

        // THEN it runs `vllm-mlx serve` on that folder, on 127.0.0.1 and the
        // given port, with only the engine's packages importable
        assert_eq!(
            args,
            [
                "-S",
                "-m",
                "vllm_mlx.cli",
                "serve",
                "/models/qwen-mlx",
                "--host",
                "127.0.0.1",
                "--port",
                "41234",
                "--enable-auto-tool-choice",
                "--tool-call-parser",
                "auto",
            ]
        );
        assert_eq!(
            launch.env,
            [(
                "PYTHONPATH".to_owned(),
                "/app/python/engines/mlx/site-packages".to_owned()
            )]
        );
    }

    #[test]
    fn the_overrides_name_the_engine_and_nothing_is_found_without_an_interpreter() {
        // GIVEN an engine directory and an interpreter named by the overrides
        let tmp = tempfile::tempdir().expect("tempdir");
        let engine = tmp.path().join("engine");
        std::fs::create_dir_all(engine.join("site-packages")).expect("mkdir");
        let python = tmp.path().join("python3");
        std::fs::write(&python, b"").expect("write");
        let engine_s = engine.display().to_string();
        let python_s = python.display().to_string();

        // WHEN the engine is located, with and without the interpreter
        let found = locate(None, None, |var| match var {
            ENV_ENGINE_DIR => Some(engine_s.clone()),
            ENV_PYTHON => Some(python_s.clone()),
            _ => None,
        });
        let without_python = locate(None, None, |var| match var {
            ENV_ENGINE_DIR => Some(engine_s.clone()),
            _ => None,
        });

        // THEN the overrides are taken, and no interpreter means no engine
        let found = found.expect("located");
        assert_eq!(found.program, python);
        assert_eq!(
            found.env[0].1,
            engine.join("site-packages").display().to_string()
        );
        assert!(without_python.is_none());
    }

    #[test]
    fn the_bundled_engine_is_found_beside_the_bundled_interpreter() {
        // GIVEN an executable whose bundle carries the interpreter and the engine
        let tmp = tempfile::tempdir().expect("tempdir");
        let exe_dir = tmp.path().join("MacOS");
        let python_root = tmp.path().join("Resources/python");
        std::fs::create_dir_all(&exe_dir).expect("mkdir");
        std::fs::create_dir_all(python_root.join("bin")).expect("mkdir");
        std::fs::create_dir_all(python_root.join("engines/mlx/site-packages")).expect("mkdir");
        std::fs::write(python_root.join(BUNDLED_INTERPRETER), b"").expect("write");

        // WHEN the engine is located with no override and an empty store
        let found =
            locate(Some(&exe_dir), Some(&tmp.path().join("store")), |_| None).expect("located");

        // THEN the bundled interpreter runs the bundled engine
        assert!(found.program.ends_with("python/bin/python3.13"));
        assert!(found.env[0].1.ends_with("engines/mlx/site-packages"));
    }
}
