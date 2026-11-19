//! Where a recomp's releases live: GitHub, GitLab, or a Forgejo (or Gitea)
//! server of the project's own. The site's list says which in `host`; a
//! Forgejo project's repo names its server first: `git.example.dev/Owner/Name`.

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Forge<'a> {
    GitHub(&'a str),
    /// On gitlab.com; the path can have subgroups (`group/sub/project`).
    GitLab(&'a str),
    Forgejo { server: &'a str, path: &'a str },
}

/// A part of a path or a server name: plain characters only, so a catalog
/// value can never turn into another address.
fn plain(part: &str) -> bool {
    !part.is_empty() && part != "." && part != ".." && part.chars().all(|c| c.is_ascii_alphanumeric() || "-_.".contains(c))
}

fn plain_path(path: &str, parts: std::ops::RangeInclusive<usize>) -> bool {
    let all: Vec<&str> = path.split('/').collect();
    parts.contains(&all.len()) && all.iter().all(|p| plain(p))
}

/// The project's place, or `None` when the repo doesn't read as one.
pub fn of<'a>(host: &str, repo: &'a str) -> Option<Forge<'a>> {
    match host.to_ascii_lowercase().as_str() {
        "gitlab" => plain_path(repo, 2..=4).then_some(Forge::GitLab(repo)),
        "forgejo" | "gitea" => {
            let (server, path) = repo.split_once('/')?;
            (plain(server) && server.contains('.') && plain_path(path, 2..=2)).then_some(Forge::Forgejo { server, path })
        }
        _ => plain_path(repo, 2..=2).then_some(Forge::GitHub(repo)),
    }
}

impl Forge<'_> {
    /// For the player: "GitHub", "GitLab", or the server's name.
    pub fn name(&self) -> String {
        match self {
            Forge::GitHub(_) => "GitHub".into(),
            Forge::GitLab(_) => "GitLab".into(),
            Forge::Forgejo { server, .. } => (*server).into(),
        }
    }

    pub fn page(&self) -> String {
        match self {
            Forge::GitHub(repo) => format!("https://github.com/{repo}"),
            Forge::GitLab(path) => format!("https://gitlab.com/{path}"),
            Forge::Forgejo { server, path } => format!("https://{server}/{path}"),
        }
    }

    /// A release file's address from its tag and name, where the server has
    /// one. GitLab's files are links of any shape, known only through its API.
    pub fn release_file(&self, tag: &str, name: &str) -> Option<String> {
        let name = name.replace(' ', "%20");
        match self {
            Forge::GitLab(_) => None,
            _ => Some(format!("{}/releases/download/{tag}/{name}", self.page())),
        }
    }

    /// The newest releases, newest first.
    pub fn releases_api(&self) -> String {
        match self {
            Forge::GitHub(repo) => format!("https://api.github.com/repos/{repo}/releases?per_page=10"),
            Forge::GitLab(path) => format!("https://gitlab.com/api/v4/projects/{}/releases?per_page=10", path.replace('/', "%2F")),
            Forge::Forgejo { server, path } => format!("https://{server}/api/v1/repos/{path}/releases?limit=10"),
        }
    }

    /// The file name its release info is kept under on disk.
    pub fn cache_name(&self) -> String {
        match self {
            // As before, so the copies already on disk still count.
            Forge::GitHub(repo) => repo.replace('/', "__"),
            Forge::GitLab(path) => format!("gitlab__{}", path.replace('/', "__")),
            Forge::Forgejo { server, path } => format!("{server}__{}", path.replace('/', "__")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_each_kind_of_project() {
        let hub = of("", "Zelda64Recomp/Zelda64Recomp").unwrap();
        assert_eq!(hub.release_file("v1", "A B.zip").unwrap(), "https://github.com/Zelda64Recomp/Zelda64Recomp/releases/download/v1/A%20B.zip");
        let lab = of("gitlab", "sonicdcer/MarioKart64Recomp").unwrap();
        assert_eq!(lab.releases_api(), "https://gitlab.com/api/v4/projects/sonicdcer%2FMarioKart64Recomp/releases?per_page=10");
        assert_eq!(lab.release_file("v1", "a.zip"), None);
        let jo = of("forgejo", "git.onerag90.de/recomps/fe3h-recomp").unwrap();
        assert_eq!(jo.name(), "git.onerag90.de");
        assert_eq!(jo.releases_api(), "https://git.onerag90.de/api/v1/repos/recomps/fe3h-recomp/releases?limit=10");
        assert_eq!(jo.release_file("v0.2.5", "x.zip").unwrap(), "https://git.onerag90.de/recomps/fe3h-recomp/releases/download/v0.2.5/x.zip");
    }

    #[test]
    fn refuses_what_could_point_elsewhere() {
        assert_eq!(of("", "a/b/c"), None);
        assert_eq!(of("", "a/../b"), None);
        assert_eq!(of("forgejo", "owner/name"), None, "no server named");
        assert_eq!(of("forgejo", "evil.com?x=/a/b"), None);
        assert_eq!(of("gitlab", "a"), None);
    }
}
