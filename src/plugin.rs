use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::io::Write;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PluginDef {
    pub name: String,
    /// Fenced-code-block language (```<trigger>) that hands content to this plugin.
    pub trigger: String,
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct PluginInvocation {
    pub plugin_name: String,
    pub content: String,
}

#[derive(Debug, Clone)]
pub enum PluginOutput {
    Text(String),
    Error(String),
}

#[derive(Debug, Clone)]
pub enum PluginState {
    Ready(PluginOutput),
    Pending,
    NotFound,
}

type CacheKey = (String, [u8; 32]);

#[derive(Default)]
struct CacheState {
    results: HashMap<CacheKey, PluginOutput>,
    pending: HashSet<CacheKey>,
}

pub struct PluginHost {
    plugins: Vec<PluginDef>,
    state: Arc<Mutex<CacheState>>,
}

impl PluginHost {
    pub fn new(plugins: Vec<PluginDef>) -> Self {
        Self {
            plugins,
            state: Arc::new(Mutex::new(CacheState::default())),
        }
    }

    pub fn find_by_trigger(&self, trigger: &str) -> Option<&PluginDef> {
        self.plugins.iter().find(|p| p.trigger == trigger)
    }

    /// Returns the cached result if available, the `Pending` state if a render
    /// is already in flight for this (plugin, content) pair, otherwise kicks
    /// off an async subprocess invocation and returns `Pending`.
    pub fn query(&self, plugin_name: &str, content: &str) -> PluginState {
        let Some(def) = self
            .plugins
            .iter()
            .find(|p| p.name == plugin_name)
            .cloned()
        else {
            return PluginState::NotFound;
        };
        let hash: [u8; 32] = Sha256::digest(content.as_bytes()).into();
        let key: CacheKey = (plugin_name.to_string(), hash);
        {
            let st = self.state.lock().unwrap();
            if let Some(r) = st.results.get(&key) {
                return PluginState::Ready(r.clone());
            }
            if st.pending.contains(&key) {
                return PluginState::Pending;
            }
        }
        {
            let mut st = self.state.lock().unwrap();
            st.pending.insert(key.clone());
        }
        let state = Arc::clone(&self.state);
        let content = content.to_string();
        let key_cb = key.clone();
        thread::spawn(move || {
            let result = run(&def, &content);
            let mut st = state.lock().unwrap();
            st.pending.remove(&key_cb);
            st.results.insert(key_cb, result);
        });
        PluginState::Pending
    }
}

fn run(def: &PluginDef, content: &str) -> PluginOutput {
    let mut cmd = Command::new(&def.command);
    cmd.args(&def.args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            return PluginOutput::Error(format!(
                "failed to spawn `{}`: {e} (is it installed?)",
                def.command
            ));
        }
    };
    if let Some(mut stdin) = child.stdin.take() {
        if let Err(e) = stdin.write_all(content.as_bytes()) {
            return PluginOutput::Error(format!("write stdin: {e}"));
        }
    }
    match child.wait_with_output() {
        Ok(out) if out.status.success() => {
            PluginOutput::Text(String::from_utf8_lossy(&out.stdout).into_owned())
        }
        Ok(out) => PluginOutput::Error(
            String::from_utf8_lossy(&out.stderr)
                .trim()
                .to_string(),
        ),
        Err(e) => PluginOutput::Error(format!("wait: {e}")),
    }
}

/// Built-in plugin roster used when the user's config file omits `[[plugins]]`
/// or can't be loaded. User config entries override / extend this list.
pub fn default_plugins() -> Vec<PluginDef> {
    vec![
        // Reference plugin: echoes stdin in uppercase. Requires `tr` (POSIX).
        // Use ```shout as the fence lang to trigger it.
        PluginDef {
            name: "shout".into(),
            trigger: "shout".into(),
            command: "tr".into(),
            args: vec!["a-z".into(), "A-Z".into()],
        },
        // Mermaid: renders to SVG on stdout when the `mmdc` CLI is installed.
        // The SVG text is shown inline until bitmap rendering lands.
        PluginDef {
            name: "mermaid".into(),
            trigger: "mermaid".into(),
            command: "mmdc".into(),
            args: vec![
                "-i".into(),
                "-".into(),
                "-o".into(),
                "/dev/stdout".into(),
                "-q".into(),
            ],
        },
    ]
}
