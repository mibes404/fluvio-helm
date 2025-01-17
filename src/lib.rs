use fluvio_command::CommandExt;
use serde::Deserialize;
use std::{path::PathBuf, process::Command};
use tracing::{instrument, warn};

pub use crate::error::HelmError;

mod error;

const HTTPS_PROXY_ENV: &str = "HTTPS_PROXY";
const NO_PROXY_ENV: &str = "NO_PROXY";

/// Installer Argument
#[derive(Debug)]
pub struct InstallArg {
    pub name: String,
    pub chart: String,
    pub version: Option<String>,
    pub namespace: Option<String>,
    pub opts: Vec<(String, String)>,
    pub values: Vec<PathBuf>,
    pub develop: bool,
}

impl InstallArg {
    pub fn new<N: Into<String>, C: Into<String>>(name: N, chart: C) -> Self {
        Self {
            name: name.into(),
            chart: chart.into(),
            version: None,
            namespace: None,
            opts: vec![],
            values: vec![],
            develop: false,
        }
    }

    /// set chart version
    pub fn version<S: Into<String>>(mut self, version: S) -> Self {
        self.version = Some(version.into());
        self
    }

    /// set namepsace
    pub fn namespace<S: Into<String>>(mut self, ns: S) -> Self {
        self.namespace = Some(ns.into());
        self
    }

    /// reset array of options
    pub fn opts(mut self, options: Vec<(String, String)>) -> Self {
        self.opts = options;
        self
    }

    /// set a single option
    pub fn opt<K: Into<String>, V: Into<String>>(mut self, key: K, value: V) -> Self {
        self.opts.push((key.into(), value.into()));
        self
    }

    /// set to use develop
    pub fn develop(mut self) -> Self {
        self.develop = true;
        self
    }

    /// set list of values
    pub fn values(mut self, values: Vec<PathBuf>) -> Self {
        self.values = values;
        self
    }

    /// set one value
    pub fn value(&mut self, value: PathBuf) -> &mut Self {
        self.values.push(value);
        self
    }

    pub fn install(&self) -> Command {
        let mut command = Command::new("helm");
        command.args(["install", &self.name, &self.chart]);
        self.apply_args(&mut command);
        command
    }

    pub fn upgrade(&self) -> Command {
        let mut command = Command::new("helm");
        command.args(["upgrade", "--install", &self.name, &self.chart]);
        self.apply_args(&mut command);
        command
    }

    fn apply_args(&self, command: &mut Command) {
        if let Some(namespace) = &self.namespace {
            command.args(["--namespace", namespace]);
        }

        if self.develop {
            command.arg("--devel");
        }

        if let Some(version) = &self.version {
            command.args(["--version", version]);
        }

        for value_path in &self.values {
            command.arg("--values").arg(value_path);
        }

        for (key, val) in &self.opts {
            command.arg("--set").arg(format!("{}={}", key, val));
        }
    }
}

impl From<InstallArg> for Command {
    fn from(arg: InstallArg) -> Self {
        let mut command = Command::new("helm");
        command.args(["install", &arg.name, &arg.chart]);

        if let Some(namespace) = &arg.namespace {
            command.args(["--namespace", namespace]);
        }

        if arg.develop {
            command.arg("--devel");
        }

        if let Some(version) = &arg.version {
            command.args(["--version", version]);
        }

        for value_path in &arg.values {
            command.arg("--values").arg(value_path);
        }

        for (key, val) in &arg.opts {
            command.arg("--set").arg(format!("{}={}", key, val));
        }

        command
    }
}

/// Uninstaller Argument
#[derive(Debug)]
pub struct UninstallArg {
    pub release: String,
    pub namespace: Option<String>,
    pub ignore_not_found: bool,
    pub dry_run: bool,
    pub timeout: Option<String>,
}

impl UninstallArg {
    pub fn new(release: String) -> Self {
        Self {
            release,
            namespace: None,
            ignore_not_found: false,
            dry_run: false,
            timeout: None,
        }
    }

    /// set namepsace
    pub fn namespace(mut self, ns: String) -> Self {
        self.namespace = Some(ns);
        self
    }

    /// set ignore not found
    pub fn ignore_not_found(mut self) -> Self {
        self.ignore_not_found = true;
        self
    }

    /// set dry tun
    pub fn dry_run(mut self) -> Self {
        self.dry_run = true;
        self
    }

    /// set timeout
    pub fn timeout(mut self, timeout: String) -> Self {
        self.timeout = Some(timeout);
        self
    }
}

impl From<UninstallArg> for Command {
    fn from(arg: UninstallArg) -> Self {
        let mut command = Command::new("helm");
        command.args(["uninstall", &arg.release]);

        if let Some(namespace) = &arg.namespace {
            command.args(["--namespace", namespace]);
        }

        if arg.dry_run {
            command.arg("--dry-run");
        }

        for timeout in &arg.timeout {
            command.arg("--timeout").arg(timeout);
        }

        command
    }
}

/// Client to manage helm operations
#[derive(Debug)]
#[non_exhaustive]
pub struct HelmClient {
    config: Config,
}

impl HelmClient {
    /// Creates a Rust client to manage our helm needs.
    ///
    /// This only succeeds if the helm command can be found.
    pub fn new() -> Result<Self, HelmError> {
        Self::new_with_config(Config::default())
    }

    fn new_with_config(config: Config) -> Result<Self, HelmError> {
        let output = Command::new("helm").arg("version").result()?;

        // Convert command output into a string
        let out_str = String::from_utf8(output.stdout).map_err(HelmError::Utf8Error)?;

        // Check that the version command gives a version.
        // In the future, we can parse the version string and check
        // for compatible CLI client version.
        if !out_str.contains("version") {
            return Err(HelmError::HelmVersionNotFound(out_str));
        }

        // If checks succeed, create Helm client
        Ok(Self { config })
    }

    /// Creates a HelmClientBuilder to configure a HelmClient.
    pub fn builder() -> HelmClientBuilder {
        HelmClientBuilder::new()
    }

    /// Installs the given chart under the given name.
    ///
    #[instrument(skip(self))]
    pub fn install(&self, args: &InstallArg) -> Result<(), HelmError> {
        let mut command = args.install();
        self.apply_config(&mut command);
        command.result()?;
        Ok(())
    }

    /// Upgrades the given chart
    #[instrument(skip(self))]
    pub fn upgrade(&self, args: &InstallArg) -> Result<(), HelmError> {
        let mut command = args.upgrade();
        self.apply_config(&mut command);
        command.result()?;
        Ok(())
    }

    /// Uninstalls specified chart library
    pub fn uninstall(&self, uninstall: UninstallArg) -> Result<(), HelmError> {
        if uninstall.ignore_not_found {
            let app_charts = self
                .get_installed_chart_by_name(&uninstall.release, uninstall.namespace.as_deref())?;
            if app_charts.is_empty() {
                warn!("Chart does not exists, {}", &uninstall.release);
                return Ok(());
            }
        }
        let mut command: Command = uninstall.into();
        self.apply_config(&mut command);
        command.result()?;
        Ok(())
    }

    /// Adds a new helm repo with the given chart name and chart location
    #[instrument(skip(self))]
    pub fn repo_add(&self, chart: &str, location: &str) -> Result<(), HelmError> {
        let mut command = Command::new("helm");
        self.apply_config(&mut command);
        command.args(["repo", "add", chart, location]).result()?;
        Ok(())
    }

    /// Updates the local helm repository
    #[instrument(skip(self))]
    pub fn repo_update(&self) -> Result<(), HelmError> {
        let mut command = Command::new("helm");
        self.apply_config(&mut command);
        command.args(["repo", "update"]).result()?;
        Ok(())
    }

    /// Searches the repo for the named helm chart
    #[instrument(skip(self))]
    pub fn search_repo(&self, chart: &str, version: &str) -> Result<Vec<Chart>, HelmError> {
        let mut command = Command::new("helm");
        self.apply_config(&mut command);
        command
            .args(["search", "repo", chart])
            .args(["--version", version])
            .args(["--output", "json"]);

        let output = command.result()?;

        check_helm_stderr(output.stderr)?;
        serde_json::from_slice(&output.stdout).map_err(HelmError::Serde)
    }

    /// Get all the available versions
    #[instrument(skip(self))]
    pub fn versions(&self, chart: &str) -> Result<Vec<Chart>, HelmError> {
        let mut command = Command::new("helm");
        self.apply_config(&mut command);
        command
            .args(["search", "repo"])
            .args(["--versions", chart])
            .args(["--output", "json", "--devel"]);
        let output = command.result()?;

        check_helm_stderr(output.stderr)?;
        serde_json::from_slice(&output.stdout).map_err(HelmError::Serde)
    }

    /// Checks that a given version of a given chart exists in the repo.
    #[instrument(skip(self))]
    pub fn chart_version_exists(&self, name: &str, version: &str) -> Result<bool, HelmError> {
        let versions = self.search_repo(name, version)?;
        let count = versions
            .iter()
            .filter(|chart| chart.name == name && chart.version == version)
            .count();
        Ok(count > 0)
    }

    /// Returns the list of installed charts by name
    #[instrument(skip(self))]
    pub fn get_installed_chart_by_name(
        &self,
        name: &str,
        namespace: Option<&str>,
    ) -> Result<Vec<InstalledChart>, HelmError> {
        let exact_match = format!("^{}$", name);
        let mut command = Command::new("helm");
        self.apply_config(&mut command);
        command
            .arg("list")
            .arg("--filter")
            .arg(exact_match)
            .arg("--output")
            .arg("json");

        match namespace {
            Some(ns) => {
                command.args(["--namespace", ns]);
            }
            None => {
                // Search all namespaces
                command.args(["-A"]);
            }
        }

        let output = command.result()?;
        check_helm_stderr(output.stderr)?;
        serde_json::from_slice(&output.stdout).map_err(HelmError::Serde)
    }

    /// get helm package version
    #[instrument(skip(self))]
    pub fn get_helm_version(&self) -> Result<String, HelmError> {
        let mut command = Command::new("helm");
        self.apply_config(&mut command);
        let helm_version = command
            .arg("version")
            .arg("--short")
            .output()
            .map_err(HelmError::HelmNotInstalled)?;
        let version_text = String::from_utf8(helm_version.stdout).map_err(HelmError::Utf8Error)?;
        Ok(version_text[1..].trim().to_string())
    }

    fn apply_config(&self, command: &mut Command) {
        if let Some(no_proxy) = self.config.no_proxy.as_deref() {
            command.env(NO_PROXY_ENV, no_proxy);
        }

        if let Some(https_proxy) = self.config.https_proxy.as_deref() {
            command.env(HTTPS_PROXY_ENV, https_proxy);
        }
    }
}

/// A HelmClientBuilder can be used to create a HelmClient with custom configuration.
#[derive(Debug, Default)]
pub struct HelmClientBuilder {
    config: Config,
}

#[derive(Debug, Default)]
struct Config {
    https_proxy: Option<String>,
    no_proxy: Option<String>,
}

impl HelmClientBuilder {
    /// Creates a new HelmClientBuilder.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the HTTPS_PROXY environment variable to use for helm operations.
    pub fn https_proxy(mut self, https_proxy: &str) -> Self {
        self.config.https_proxy = Some(https_proxy.to_string());
        self
    }

    /// Sets the NO_PROXY environment variable to use for helm operations.
    pub fn no_proxy(mut self, no_proxy: &str) -> Self {
        self.config.no_proxy = Some(no_proxy.to_string());
        self
    }

    /// Creates a new HelmClient with the given configuration.
    pub fn build(self) -> Result<HelmClient, HelmError> {
        HelmClient::new_with_config(self.config)
    }
}

/// Check for errors in Helm's stderr output
///
/// Returns `Ok(())` if everything is fine, or `HelmError` if something is wrong
fn check_helm_stderr(stderr: Vec<u8>) -> Result<(), HelmError> {
    if !stderr.is_empty() {
        let stderr = String::from_utf8(stderr)?;
        if stderr.contains("Kubernetes cluster unreachable") {
            return Err(HelmError::FailedToConnect);
        }
    }

    Ok(())
}

/// A representation of a chart definition in a repo.
#[derive(Debug, Deserialize)]
pub struct Chart {
    /// The chart name
    name: String,
    /// The chart version
    version: String,
}

impl Chart {
    pub fn version(&self) -> &str {
        &self.version
    }
    pub fn name(&self) -> &str {
        &self.name
    }
}

/// A representation of an installed chart.
#[derive(Debug, Deserialize)]
pub struct InstalledChart {
    /// The chart name
    pub name: String,
    /// The version of the app this chart installed
    pub app_version: String,
    /// The chart revision
    pub revision: String,
    /// Date/time when the chart was last updated
    pub updated: String,
    /// Status of the installed chart
    pub status: String,
    /// The ID of the chart that is installed
    pub chart: String,
    /// The namespace where the chart is installed (available in Helm v3.7.2+)
    pub namespace: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_get_installed_charts() {
        const JSON_RESPONSE: &str = r#"[{"name":"test_chart","namespace":"default","revision":"2","updated":"2022-09-15 14:32:56.455129736 +0000 UTC","status":"deployed","chart":"test_chart-1.2.32-rc2","app_version":"1.2.32-rc2"},{"name":"influxdb","namespace":"default","revision":"1","updated":"2022-09-12 07:54:44.505296974 +0000 UTC","status":"deployed","chart":"fluvio-influxdb2-0.0.2","app_version":"2.4.0"}]"#;
        let installed_charts: Vec<InstalledChart> =
            serde_json::from_slice(JSON_RESPONSE.as_bytes()).expect("can not parse json");
        assert_eq!(installed_charts.len(), 2);
        let test_chart = installed_charts
            .get(0)
            .expect("can not grab the first result");
        assert_eq!(test_chart.name, "test_chart");
        assert_eq!(test_chart.chart, "test_chart-1.2.32-rc2");
        assert_eq!(test_chart.app_version, "1.2.32-rc2");
        assert_eq!(test_chart.revision, "2");
        assert_eq!(test_chart.status, "deployed");
        assert_eq!(test_chart.namespace, "default");
    }

    #[test]
    fn can_construct_a_client_with_config() {
        const EXAMPLE_HOST: &str = "https://example.com";
        const LOCAL_HOST: &str = "localhost";

        let client_result = HelmClientBuilder::new()
            .https_proxy(EXAMPLE_HOST)
            .no_proxy(LOCAL_HOST)
            .build();

        match client_result {
            Ok(client) => {
                assert_eq!(client.config.https_proxy.as_deref(), Some(EXAMPLE_HOST));
                assert_eq!(client.config.no_proxy.as_deref(), Some(LOCAL_HOST));
            }
            Err(_e) => {
                // OK. This can happen when the machine on which we run the test does not have helm installed.
            }
        }
    }
}
