/// IPC message types for communication between KitsuneEngine processes.
///
/// All messages are serializable and carry a unique correlation ID for
/// request-response tracking. Messages are the only way sandboxed processes
/// can interact with privileged resources.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Unique identifier for a process in the KitsuneEngine process tree.
#[derive(Debug, Clone, Hash, Eq, PartialEq, Serialize, Deserialize)]
pub struct ProcessId(pub String);

impl std::fmt::Display for ProcessId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// The role of a process in the KitsuneEngine architecture.
#[derive(Debug, Clone, Copy, Hash, Eq, PartialEq, Serialize, Deserialize)]
pub enum ProcessRole {
    Broker,
    Network,
    Renderer,
    Js,
    Agent,
}

impl From<String> for ProcessRole {
    fn from(s: String) -> Self {
        match s.to_lowercase().as_str() {
            "broker" => ProcessRole::Broker,
            "network" => ProcessRole::Network,
            "renderer" => ProcessRole::Renderer,
            "js" | "javascript" => ProcessRole::Js,
            "agent" => ProcessRole::Agent,
            _ => ProcessRole::Renderer, // default fallback
        }
    }
}

/// Unique identifier for correlating request-response pairs.
#[derive(Debug, Clone, Hash, Eq, PartialEq, Serialize, Deserialize)]
pub struct CorrelationId(pub Uuid);

impl CorrelationId {
    /// Generate a new unique correlation ID.
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for CorrelationId {
    fn default() -> Self {
        Self::new()
    }
}

/// The privilege level of a process, determining what resources it can access.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PrivilegeLevel {
    /// Full access to vault, HIL, and IPC bus. Only the broker process.
    Broker,
    /// Can request vault data through HIL gates. Agent processes.
    SemiPrivileged,
    /// No direct access to vault or network. Renderer/JS processes.
    Sandboxed,
}

/// Process capability flags — granular permissions for IPC message routing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ProcessCapability {
    /// Can read from the privacy vault (via HIL gate).
    VaultRead,
    /// Can write to the privacy vault.
    VaultWrite,
    /// Can make outbound network requests.
    NetworkAccess,
    /// Can interact with the DOM.
    DomAccess,
    /// Can trigger HIL confirmations.
    HilTrigger,
    /// Can spawn child processes.
    ProcessSpawn,
    /// Can access the agent runtime.
    AgentRuntime,
    /// Can read/write the local filesystem (heavily restricted).
    FileSystemAccess,
}

/// An IPC message envelope — wraps all inter-process communication.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IpcMessage {
    /// Unique ID for this message for correlation.
    pub correlation_id: CorrelationId,
    /// The sending process.
    pub sender: ProcessId,
    /// The target process.
    pub target: ProcessId,
    /// The message payload.
    pub payload: IpcPayload,
    /// Timestamp of message creation.
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

impl IpcMessage {
    /// Create a new IPC message.
    pub fn new(sender: ProcessId, target: ProcessId, payload: IpcPayload) -> Self {
        Self {
            correlation_id: CorrelationId::new(),
            sender,
            target,
            payload,
            timestamp: chrono::Utc::now(),
        }
    }

    /// Create a response to this message.
    pub fn respond(&self, sender: ProcessId, payload: IpcPayload) -> Self {
        Self {
            correlation_id: self.correlation_id.clone(),
            sender,
            target: self.sender.clone(),
            payload,
            timestamp: chrono::Utc::now(),
        }
    }
}

/// The payload of an IPC message — defines what action is being requested or reported.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum IpcPayload {
    // --- Vault operations (Sandboxed → Broker) ---
    /// Request a credential from the vault.
    VaultRequest {
        key: String,
        purpose: String,
    },
    /// Vault response with granted access (never contains raw secrets).
    VaultResponse {
        granted: bool,
        token_handle: Option<String>,
        metadata: Option<String>,
    },

    // --- Network operations (Sandboxed → Network Process) ---
    /// Request to fetch a URL.
    NetworkFetchRequest {
        url: String,
        method: String,
        headers: Vec<(String, String)>,
        body: Option<Vec<u8>>,
    },
    /// Response from a network fetch.
    NetworkFetchResponse {
        status: u16,
        headers: Vec<(String, String)>,
        body: Vec<u8>,
    },

    // --- HIL operations (Agent → Broker) ---
    /// Request a human-in-the-loop confirmation.
    HilCheckpointRequest {
        action_description: String,
        trigger_class: String,
        cost: Option<String>,
        data_involved: Vec<String>,
    },
    /// HIL confirmation result.
    HilCheckpointResponse {
        approved: bool,
        approval_token: Option<String>,
    },

    // --- DOM operations (Agent → Renderer) ---
    /// Query DOM elements.
    DomQuery {
        selector: String,
    },
    /// DOM query result.
    DomQueryResult {
        elements: Vec<DomElementSummary>,
    },
    /// Fill a form field.
    DomFillField {
        selector: String,
        value_token: String, // Token handle, never raw value
    },
    /// Click an element.
    DomClick {
        selector: String,
    },
    /// DOM operation result.
    DomOperationResult {
        success: bool,
        error: Option<String>,
    },
    /// Request the renderer to display a visual tracking highlight.
    SetDomHighlight(DomHighlight),
    /// Request the renderer to clear a specific tracking highlight.
    ClearDomHighlight(String),
    /// Request the renderer to clear all tracking highlights.
    ClearAllDomHighlights,

    // --- Navigation (Agent → Broker) ---
    /// Navigate to a URL.
    NavigateRequest {
        url: String,
    },
    /// Navigation result.
    NavigateResponse {
        success: bool,
        final_url: String,
        title: Option<String>,
    },

    // --- Process lifecycle ---
    /// Process registration with the broker.
    ProcessRegister {
        privilege_level: PrivilegeLevel,
        capabilities: Vec<ProcessCapability>,
    },
    /// Acknowledgment of process registration.
    ProcessRegistered {
        assigned_id: String,
    },
    /// Process shutdown signal.
    ProcessShutdown {
        reason: String,
    },

    // --- Agent operations ---
    /// Agent action request.
    AgentActionRequest {
        agent_id: String,
        action: String,
        parameters: serde_json::Value,
    },
    /// Agent action result.
    AgentActionResult {
        success: bool,
        result: serde_json::Value,
        cost_incurred: Option<String>,
    },

    // --- Error ---
    /// Error response for any failed operation.
    Error {
        code: String,
        message: String,
    },
}

/// Summary of a DOM element returned via IPC (never contains sensitive data).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DomElementSummary {
    /// CSS selector path to this element.
    pub selector_path: String,
    /// Tag name (e.g., "input", "button", "a").
    pub tag_name: String,
    /// Element ID if present.
    pub id: Option<String>,
    /// CSS classes on the element.
    pub classes: Vec<String>,
    /// Text content (truncated for safety).
    pub text_content: Option<String>,
    /// Key attributes (type, name, placeholder — never value for inputs).
    pub attributes: Vec<(String, String)>,
    /// Whether this element is visible.
    pub visible: bool,
    /// Bounding box in viewport coordinates.
    pub bounding_rect: Option<BoundingRect>,
}

/// A bounding rectangle in viewport coordinates.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct BoundingRect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum HighlightStyle {
    Reading,
    Acting,
    Done,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum HighlightPhase {
    FadingIn,
    Active,
    Pulsing,
    FadingOut,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct HighlightRect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

fn default_phase_time() -> Option<std::time::Instant> {
    None // Render loops will instantiate true Instant on deserialize side
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DomHighlight {
    pub element_id: String,
    pub rect: HighlightRect,
    pub style: HighlightStyle,
    pub phase: HighlightPhase,
    /// Internal renderer clock instantiation boundary. Not serialized.
    #[serde(skip, default = "default_phase_time")]
    pub phase_start: Option<std::time::Instant>,
}
