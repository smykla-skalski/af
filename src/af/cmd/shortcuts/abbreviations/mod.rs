use crate::consts::HEAD;
use crate::consts::{
    CHECKOUT, DIFF, FETCH, FF_ONLY, FORCE, FORCE_WITH_LEASE, GIT, MERGE, NO_VERIFY, ORIGIN_SLICE,
    ORIGIN_UPSTREAM_SLICE, PBCOPY, PUSH, UPSTREAM_ORIGIN_SLICE, UPSTREAM_SLICE,
};
use anyhow::bail;
use clap::{Subcommand, ValueEnum};
use git2::Repository;
use log::debug;
use std::fmt::Debug;
use std::{iter, slice, vec};

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, ValueEnum)]
pub enum GitPushRemote {
    #[default]
    Origin,
    OriginFirst,
    Upstream,
    UpstreamFirst,
}

impl IntoIterator for GitPushRemote {
    type Item = &'static str;
    type IntoIter = iter::Copied<slice::Iter<'static, &'static str>>;

    fn into_iter(self) -> Self::IntoIter {
        match self {
            GitPushRemote::Upstream => UPSTREAM_SLICE.iter().copied(),
            GitPushRemote::UpstreamFirst => UPSTREAM_ORIGIN_SLICE.iter().copied(),
            GitPushRemote::Origin => ORIGIN_SLICE.iter().copied(),
            GitPushRemote::OriginFirst => ORIGIN_UPSTREAM_SLICE.iter().copied(),
        }
    }
}

#[derive(Debug, Subcommand)]
pub enum Abbreviation {
    /// Expands to: git checkout <default-branch> && git fetch <remote> && git merge --ff-only <remote>/<default-branch>
    #[command(name = "gcmff")]
    GitCheckoutMasterFetchFastForward,

    /// Expands to: git push <remote> <branch> [optional flags]
    #[command(name = "gp")]
    GitPush {
        /// Remote priority strategy when pushing (e.g., upstream first, origin first)
        #[arg(long = "remote", short, value_enum, default_value_t)]
        remote_priority: GitPushRemote,

        /// Push with --no-verify flag (skip pre-push hooks)
        #[arg(long, short)]
        no_verify: bool,

        /// Push with --force-with-lease flag (safe force push)
        #[arg(long, short)]
        force_with_lease: bool,

        /// Push with --force flag (unsafe force push, overrides remote)
        #[arg(long, short = 'F', conflicts_with = "force_with_lease")]
        force: bool,
    },

    /// Expands to: git diff <reference> -- <files> [optionally copy to clipboard]
    #[command(name = "gd")]
    GitDiff {
        /// Specific files to diff (if omitted, diffs all changes)
        #[arg(long, short)]
        files: Option<Vec<String>>,

        /// Git reference to diff against (e.g. HEAD)
        #[arg(long, short)]
        reference: Option<String>,

        /// Copy the diff output to clipboard using pbcopy
        #[arg(long, short)]
        pbcopy: bool,
    },
}

impl Abbreviation {
    pub fn run(&self) {
        match self {
            Abbreviation::GitCheckoutMasterFetchFastForward => match Repository::open_from_env() {
                Ok(repo) => match get_remote_and_default_branch(&repo, UPSTREAM_ORIGIN_SLICE) {
                    Ok((remote, default_branch)) => {
                        let head = repo
                            .head()
                            .ok()
                            .and_then(|h| h.shorthand().ok().map(str::to_string))
                            .unwrap_or_default();

                        if head != default_branch {
                            print!("{GIT} {CHECKOUT} {default_branch} && ");
                        }

                        print!(
                            "{GIT} {FETCH} {remote} && {GIT} {MERGE} {FF_ONLY} {remote}/{default_branch}"
                        );
                    }
                    Err(err) => debug!("Failed to get remote and default branch: {err:#}"),
                },
                Err(err) => debug!("Failed to open repository from environment: {err:#}"),
            },
            Abbreviation::GitPush {
                remote_priority,
                no_verify,
                force_with_lease,
                force,
            } => match Repository::open_from_env() {
                Ok(repo) => {
                    let found_remote = get_existing_remote(&repo, *remote_priority);

                    if let Some(remote) = found_remote {
                        match repo.head() {
                            Ok(head) if head.is_branch() => {
                                if let Ok(branch) = head.shorthand() {
                                    let mut cmd = vec![GIT, PUSH, &remote, branch];

                                    if *no_verify {
                                        cmd.push(NO_VERIFY);
                                    }

                                    if *force_with_lease {
                                        cmd.push(FORCE_WITH_LEASE);
                                    } else if *force {
                                        cmd.push(FORCE);
                                    }

                                    return print!("{}", cmd.join(" "));
                                }

                                debug!("Branch name could not be determined");
                            }
                            Ok(_) => debug!("{HEAD} is not pointing to a branch"),
                            Err(err) => debug!("Failed to get {HEAD}: {err:#}"),
                        }
                    } else {
                        debug!("No valid remote found from priority list");
                    }
                }
                Err(err) => debug!("Failed to open repository from environment: {err:#}"),
            },
            Abbreviation::GitDiff {
                files,
                reference,
                pbcopy,
            } => {
                if Repository::open_from_env().is_err() {
                    debug!("Failed to open repository from environment");
                    return;
                }

                let mut cmd = vec![GIT, DIFF];

                if let Some(ref_name) = reference {
                    cmd.push(ref_name);
                }

                if let Some(files) = files {
                    cmd.push("--");
                    cmd.extend(files.iter().map(String::as_str));
                }

                if *pbcopy {
                    cmd.extend(vec!["|", PBCOPY]);
                }

                print!("{}", cmd.join(" "))
            }
        }
    }
}

#[derive(Debug, Subcommand)]
pub enum Shortcut {
    /// Group of abbreviation subcommands (alias: a, abbr, abbreviation)
    #[command(visible_aliases = ["a", "abbr", "abbreviation"])]
    #[command(subcommand)]
    Abbreviations(Abbreviation),
}

impl Shortcut {
    pub fn run(&self) {
        match self {
            // Delegates to the selected abbreviation command
            Shortcut::Abbreviations(cmd) => cmd.run(),
        }
    }
}

/// Iterates over the provided remote names and returns the first one that exists in the repository.
fn get_existing_remote<I, S>(repo: &Repository, remote_names: I) -> Option<String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    remote_names
        .into_iter()
        .find(|name| repo.find_remote(name.as_ref()).is_ok())
        .map(|s| s.as_ref().to_string())
}

/// Returns the first found remote (from provided list) and its default branch name
fn get_remote_and_default_branch<I, S>(
    repo: &Repository,
    remote_names: I,
) -> anyhow::Result<(String, String)>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str> + Debug,
{
    for remote_name in remote_names {
        let remote_name = remote_name.as_ref();
        debug!("Trying remote: {remote_name:?}");

        if repo.find_remote(remote_name).is_ok() {
            let clean_pattern = format!("{remote_name}/");
            let head_spec = format!("{clean_pattern}{HEAD}");

            // Try HEAD first
            if let Ok((_, Some(reference))) = repo.revparse_ext(&head_spec)
                && let Ok(ref_short) = reference.shorthand() {
                    return Ok((
                        remote_name.to_string(),
                        ref_short.replace(&clean_pattern, ""),
                    ));
                }

            // Fallback: check for "main" and "master" branches
            for branch in ["main", "master"] {
                let branch_spec = format!("{clean_pattern}{branch}");
                if repo.revparse_single(&branch_spec).is_ok() {
                    return Ok((remote_name.to_string(), branch.to_string()));
                }
            }

            debug!("No default branch found for remote {remote_name:?}");
        } else {
            debug!("Remote {remote_name:?} not found");
        }
    }

    bail!("Could not find remote and default branch")
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn init_repo() -> (TempDir, Repository) {
        let temp_dir = TempDir::new().unwrap();
        let repo = Repository::init(temp_dir.path()).unwrap();
        (temp_dir, repo)
    }

    fn init_repo_with_initial_branch(branch: &str) -> (TempDir, Repository) {
        let temp_dir = TempDir::new().unwrap();
        let mut options = git2::RepositoryInitOptions::new();
        options.initial_head(branch);
        let repo = Repository::init_opts(temp_dir.path(), &options).unwrap();
        (temp_dir, repo)
    }

    fn commit(repo: &Repository, msg: &str) -> git2::Oid {
        let tree_id = repo.index().unwrap().write_tree().unwrap();
        let tree = repo.find_tree(tree_id).unwrap();
        let signature = git2::Signature::now("Test User", "test@example.com").unwrap();

        let parent_commit = match repo.head() {
            Ok(head) => vec![repo.find_commit(head.target().unwrap()).unwrap()],
            Err(_) => vec![],
        };
        let parents: Vec<&git2::Commit> = parent_commit.iter().collect();

        repo.commit(Some("HEAD"), &signature, &signature, msg, &tree, &parents)
            .unwrap()
    }

    #[test]
    fn test_finds_existing_remote_head() {
        let (remote_td, remote_repo) = init_repo_with_initial_branch("master");
        // Create a commit on remote so it has a HEAD
        commit(&remote_repo, "Initial commit");

        let (local_td, local_repo) = init_repo();

        // Add remote
        local_repo
            .remote("origin", remote_td.path().to_str().unwrap())
            .unwrap();
        local_repo
            .find_remote("origin")
            .unwrap()
            .fetch(
                &["refs/heads/master:refs/remotes/origin/master"],
                None,
                None,
            )
            .unwrap();

        // In a real clone, origin/HEAD is set. We simulate this.
        // refs/remotes/origin/HEAD -> refs/remotes/origin/master
        local_repo
            .reference_symbolic(
                "refs/remotes/origin/HEAD",
                "refs/remotes/origin/master",
                true,
                "simulated clone",
            )
            .unwrap();

        let (remote_name, branch) = get_remote_and_default_branch(&local_repo, ["origin"]).unwrap();
        assert_eq!(remote_name, "origin");
        assert_eq!(branch, "master");

        // Keep temp dirs alive
        drop(local_td);
        drop(remote_td);
    }

    #[test]
    fn test_fallback_to_main_if_head_missing() {
        let (remote_td, remote_repo) = init_repo_with_initial_branch("main");

        // Create 'main' branch on remote
        commit(&remote_repo, "Initial commit");

        let (local_td, local_repo) = init_repo();

        local_repo
            .remote("upstream", remote_td.path().to_str().unwrap())
            .unwrap();
        // Fetch main to refs/remotes/upstream/main
        local_repo
            .find_remote("upstream")
            .unwrap()
            .fetch(&["refs/heads/main:refs/remotes/upstream/main"], None, None)
            .unwrap();

        // Note: We do NOT set upstream/HEAD here, to simulate it missing.

        let (remote_name, branch) =
            get_remote_and_default_branch(&local_repo, ["upstream"]).unwrap();
        assert_eq!(remote_name, "upstream");
        assert_eq!(branch, "main");

        drop(local_td);
        drop(remote_td);
    }

    #[test]
    fn test_fallback_to_master_if_head_missing() {
        let (remote_td, remote_repo) = init_repo_with_initial_branch("master");
        commit(&remote_repo, "Initial commit");
        // Default is usually master for init, so we have refs/heads/master

        let (local_td, local_repo) = init_repo();

        local_repo
            .remote("upstream", remote_td.path().to_str().unwrap())
            .unwrap();
        // Fetch master to refs/remotes/upstream/master
        local_repo
            .find_remote("upstream")
            .unwrap()
            .fetch(
                &["refs/heads/master:refs/remotes/upstream/master"],
                None,
                None,
            )
            .unwrap();

        // No upstream/HEAD

        let (remote_name, branch) =
            get_remote_and_default_branch(&local_repo, ["upstream"]).unwrap();
        assert_eq!(remote_name, "upstream");
        assert_eq!(branch, "master");

        drop(local_td);
        drop(remote_td);
    }

    #[test]
    fn test_fails_if_no_matching_branch() {
        let (remote_td, remote_repo) = init_repo();
        commit(&remote_repo, "Initial commit");
        // Rename master to 'devel'
        let head_ref = remote_repo.head().unwrap();
        let commit = remote_repo.find_commit(head_ref.target().unwrap()).unwrap();
        remote_repo.branch("devel", &commit, false).unwrap();

        let (local_td, local_repo) = init_repo();

        local_repo
            .remote("origin", remote_td.path().to_str().unwrap())
            .unwrap();
        local_repo
            .find_remote("origin")
            .unwrap()
            .fetch(&["refs/heads/devel:refs/remotes/origin/devel"], None, None)
            .unwrap();

        // No HEAD, no main, no master

        let result = get_remote_and_default_branch(&local_repo, ["origin"]);
        assert!(result.is_err());

        drop(local_td);
        drop(remote_td);
    }

    #[test]
    fn test_gp_finds_remote_without_head_or_branches() {
        let (remote_td, _remote_repo) = init_repo();
        let (local_td, local_repo) = init_repo();

        // Add "upstream" remote, but no branches fetched, no HEAD.
        // Just the config entry exists.
        local_repo
            .remote("upstream", remote_td.path().to_str().unwrap())
            .unwrap();

        // Create 'origin' as well to simulate priority checking
        local_repo
            .remote("origin", remote_td.path().to_str().unwrap())
            .unwrap();

        // Case 1: Upstream first priority
        let priority = GitPushRemote::UpstreamFirst;
        // Should find "upstream" even though it has no branches
        let remote = get_existing_remote(&local_repo, priority).unwrap();
        assert_eq!(remote, "upstream");

        // Case 2: Origin first priority
        let priority = GitPushRemote::OriginFirst;
        let remote = get_existing_remote(&local_repo, priority).unwrap();
        assert_eq!(remote, "origin");

        drop(local_td);
        drop(remote_td);
    }
}
