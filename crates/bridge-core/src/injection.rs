//! Injection ledger and browser message mapping.
//!
//! Reconciles browser-native roles (`user` vs `assistant`) with authoritative
//! multi-actor logical identities. Injected external actor turns (e.g. Orbit SA)
//! pass through ChatGPT's native user composer, but must remain attributed to their
//! true logical sender rather than being mistaken for human-typed messages.

use chrono::{DateTime, Utc};
use protocol::{CorrelationId, InjectionId, ParticipantId};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// Formats an external actor message for insertion into the ChatGPT composer.
pub fn format_external_injection(display_name: &str, content: &str) -> String {
    format!("[External participant: {display_name}]\n\n{content}")
}

/// A pending outbound injection waiting for DOM reflection in ChatGPT Web.
#[derive(Debug, Clone)]
pub struct PendingInjection {
    pub injection_id: InjectionId,
    pub correlation_id: CorrelationId,
    pub actor_id: ParticipantId,
    pub snippet: String,
    pub accepted: bool,
    pub created_at: DateTime<Utc>,
}

/// Metadata stored when an injection materializes into a native DOM message.
#[derive(Debug, Clone)]
pub struct MaterializedInjection {
    pub injection_id: InjectionId,
    pub correlation_id: CorrelationId,
    pub actor_id: ParticipantId,
    pub external_message_id: String,
    pub materialized_at: DateTime<Utc>,
}

#[derive(Debug, Default)]
struct InjectionLedgerInner {
    pending_by_id: HashMap<InjectionId, PendingInjection>,
    pending_by_corr: HashMap<CorrelationId, InjectionId>,
    materialized_by_external_id: HashMap<String, MaterializedInjection>,
}

/// Ledger tracking outbound injections to reliably resolve incoming native user messages.
#[derive(Debug, Clone, Default)]
pub struct InjectionLedger {
    inner: Arc<Mutex<InjectionLedgerInner>>,
}

impl InjectionLedger {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(InjectionLedgerInner::default())),
        }
    }

    /// Record a pending injection sent to the browser.
    pub fn record_injection(
        &self,
        injection_id: InjectionId,
        correlation_id: CorrelationId,
        actor_id: ParticipantId,
        content: &str,
    ) {
        let snippet = content
            .chars()
            .take(64)
            .collect::<String>()
            .trim()
            .to_string();

        let mut lock = self.inner.lock().unwrap();
        let pending = PendingInjection {
            injection_id: injection_id.clone(),
            correlation_id: correlation_id.clone(),
            actor_id,
            snippet,
            accepted: false,
            created_at: Utc::now(),
        };

        lock.pending_by_corr
            .insert(correlation_id, injection_id.clone());
        lock.pending_by_id.insert(injection_id, pending);
    }

    /// Mark an injection as accepted by the browser extension.
    pub fn mark_accepted(&self, injection_id: &InjectionId) -> bool {
        let mut lock = self.inner.lock().unwrap();
        if let Some(pending) = lock.pending_by_id.get_mut(injection_id) {
            pending.accepted = true;
            true
        } else {
            false
        }
    }

    /// Record that an injection has materialized into a specific DOM user message.
    ///
    /// Links `injection_id` to `external_message_id` and records the mapping.
    pub fn mark_materialized(
        &self,
        injection_id: &InjectionId,
        external_message_id: &str,
    ) -> Option<ParticipantId> {
        let mut lock = self.inner.lock().unwrap();

        if let Some(pending) = lock.pending_by_id.remove(injection_id) {
            lock.pending_by_corr.remove(&pending.correlation_id);

            let actor_id = pending.actor_id.clone();
            lock.materialized_by_external_id.insert(
                external_message_id.to_string(),
                MaterializedInjection {
                    injection_id: pending.injection_id,
                    correlation_id: pending.correlation_id,
                    actor_id: actor_id.clone(),
                    external_message_id: external_message_id.to_string(),
                    materialized_at: Utc::now(),
                },
            );

            Some(actor_id)
        } else {
            None
        }
    }

    /// Resolves an incoming native user message:
    ///
    /// 1. Direct match on `external_message_id` via previously materialized injection.
    /// 2. Direct match on `correlation_id`.
    /// 3. Fallback: Content snippet match among pending injections.
    /// 4. If no match is found, returns `None` (indicating a manual human message).
    pub fn resolve_user_message(
        &self,
        external_message_id: Option<&str>,
        correlation_id: Option<&CorrelationId>,
        content: &str,
    ) -> Option<ParticipantId> {
        let mut lock = self.inner.lock().unwrap();

        // 1. Check if external_message_id was already materialized
        if let Some(ext_id) = external_message_id
            && let Some(mat) = lock.materialized_by_external_id.get(ext_id)
        {
            return Some(mat.actor_id.clone());
        }

        // 2. Direct correlation match
        if let Some(corr) = correlation_id
            && let Some(inj_id) = lock.pending_by_corr.remove(corr)
            && let Some(pending) = lock.pending_by_id.remove(&inj_id)
        {
            if let Some(ext_id) = external_message_id {
                lock.materialized_by_external_id.insert(
                    ext_id.to_string(),
                    MaterializedInjection {
                        injection_id: pending.injection_id,
                        correlation_id: pending.correlation_id,
                        actor_id: pending.actor_id.clone(),
                        external_message_id: ext_id.to_string(),
                        materialized_at: Utc::now(),
                    },
                );
            }
            return Some(pending.actor_id);
        }

        // 3. Fallback: Content snippet / prefix match for composer-reflected text
        let snippet = content.chars().take(64).collect::<String>();
        let mut matched_key = None;
        for (inj_id, pending) in lock.pending_by_id.iter() {
            if snippet.contains(&pending.snippet) || pending.snippet.contains(snippet.trim()) {
                matched_key = Some(inj_id.clone());
                break;
            }
        }

        if let Some(key) = matched_key {
            let pending = lock.pending_by_id.remove(&key).unwrap();
            lock.pending_by_corr.remove(&pending.correlation_id);

            if let Some(ext_id) = external_message_id {
                lock.materialized_by_external_id.insert(
                    ext_id.to_string(),
                    MaterializedInjection {
                        injection_id: pending.injection_id,
                        correlation_id: pending.correlation_id,
                        actor_id: pending.actor_id.clone(),
                        external_message_id: ext_id.to_string(),
                        materialized_at: Utc::now(),
                    },
                );
            }

            return Some(pending.actor_id);
        }

        None
    }

    /// Returns the number of currently pending injections.
    pub fn pending_count(&self) -> usize {
        self.inner.lock().unwrap().pending_by_id.len()
    }
}
