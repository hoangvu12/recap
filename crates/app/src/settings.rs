//! What Rust gives Settings (the UI side: `ui/host.luau`, the Providers
//! page in `ui/screens/settings/providers`).
//!
//! ```text
//! shared   providers        Providers::view(): jobs, stt, ai, presets (keys masked)
//!          provider_test    { target, status = idle|running|ok|error, latency_ms, error }
//! action   set_job          { job, slot, provider }  a job's main or backup
//!          save_provider    Draft (its key, if typed, goes to Credential Manager)
//!          remove_provider  id
//! async    test_provider    { target, draft }: runs the provider's test() on the
//!                           engine runtime; the outcome lands in provider_test
//! ```
//!
//! A later Settings page registers its own keys and actions in a module
//! like this one, called from main.rs.

use recap_providers::{Draft, Job, Providers, Slot};
use serde::Deserialize;
use zgx_script::{ScriptHost, Shared, Value};

fn value(json: serde_json::Value) -> Value {
    Value::from_json(&json.to_string()).unwrap_or(Value::Nil)
}

fn json<T: for<'de> Deserialize<'de>>(arg: &Value) -> Option<T> {
    serde_json::from_str(&arg.to_json()).map_err(|error| eprintln!("recap: settings: {error}")).ok()
}

#[derive(Deserialize)]
struct SetJob {
    job: String,
    slot: Slot,
    #[serde(default)]
    provider: Option<String>,
}

#[derive(Deserialize)]
struct TestRequest {
    target: String,
    draft: Draft,
}

/// The providers' state and actions, on `script`. Tests run on `runtime`
/// (the engine's: adapters use tokio).
#[derive(Clone)]
pub struct ProvidersHost {
    providers: Providers,
    view: Shared,
}

impl ProvidersHost {
    pub fn register(script: &ScriptHost, providers: Providers, runtime: tokio::runtime::Handle) -> Self {
        let host = Self { view: script.shared("providers", value(providers.view())), providers };
        let test_state = script.shared("provider_test", value(serde_json::json!({
            "target": "", "status": "idle", "latency_ms": 0, "error": "",
        })));

        let h = host.clone();
        script.action("set_job", move |arg| {
            let Some(SetJob { job, slot, provider }) = json::<SetJob>(&arg) else { return };
            let Some(job) = Job::from_id(&job) else { return };
            if let Err(error) = h.providers.set_job(job, slot, provider.as_deref()) {
                eprintln!("recap: {error}");
            }
            h.publish();
        });

        let h = host.clone();
        script.action("save_provider", move |arg| {
            let Some(draft) = json::<Draft>(&arg) else { return };
            if let Err(error) = h.providers.save(draft) {
                eprintln!("recap: save provider: {error}");
            }
            h.publish();
        });

        let h = host.clone();
        script.action("remove_provider", move |arg| {
            if let Value::Str(id) = &arg
                && let Err(error) = h.providers.remove(id.as_ref())
            {
                eprintln!("recap: remove provider: {error}");
            }
            h.publish();
        });

        let h = host.clone();
        script.async_action("test_provider", move |arg| {
            let (h, runtime, test_state) = (h.clone(), runtime.clone(), test_state.clone());
            async move {
                let Some(TestRequest { target, draft }) = json::<TestRequest>(&arg) else {
                    return Err("test_provider: bad request".to_string());
                };
                test_state.set(value(serde_json::json!({
                    "target": target, "status": "running", "latency_ms": 0, "error": "",
                })));
                let outcome = runtime
                    .spawn(h.providers.test(draft))
                    .await
                    .map_err(|error| format!("test_provider: {error}"))?;
                test_state.set(value(serde_json::json!({
                    "target": target,
                    "status": if outcome.ok { "ok" } else { "error" },
                    "latency_ms": outcome.latency_ms,
                    "error": outcome.error.unwrap_or_default(),
                })));
                h.publish();
                Ok(Value::Nil)
            }
        });
        host
    }

    pub fn providers(&self) -> &Providers {
        &self.providers
    }

    /// Publish the providers again.
    pub fn publish(&self) {
        self.view.set(value(self.providers.view()));
    }
}
