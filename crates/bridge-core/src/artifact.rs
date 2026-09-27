//! Structured engineering artifacts and discussion references.
//!
//! Artifacts are authoritative engineering documents distinct from casual conversational text.

use protocol::ArtifactId;
use serde::{Deserialize, Serialize};

/// High-level engineering artifact classifications.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactType {
    RequirementBrief,
    TechnicalProposal,
    Challenge,
    Resolution,
    AcceptanceDecision,
}

impl ArtifactType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::RequirementBrief => "requirement_brief",
            Self::TechnicalProposal => "technical_proposal",
            Self::Challenge => "challenge",
            Self::Resolution => "resolution",
            Self::AcceptanceDecision => "acceptance_decision",
        }
    }

    pub fn from_str_name(s: &str) -> Option<Self> {
        match s {
            "requirement_brief" => Some(Self::RequirementBrief),
            "technical_proposal" => Some(Self::TechnicalProposal),
            "challenge" => Some(Self::Challenge),
            "resolution" => Some(Self::Resolution),
            "acceptance_decision" => Some(Self::AcceptanceDecision),
            _ => None,
        }
    }
}

/// Durable reference to a structured artifact.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtifactRef {
    pub id: ArtifactId,
    pub artifact_type: ArtifactType,
    pub digest: Option<String>,
    pub orbit_artifact_id: Option<String>,
}
