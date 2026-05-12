#![allow(dead_code)]

use serde::{Deserialize, Serialize};

/// Query complexity levels for model routing
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Complexity {
    /// Simple chat, explanation, or question
    Simple,
    /// Multi-step reasoning or analysis
    Medium,
    /// Complex logic, deep reasoning, architecture design
    Complex,
    /// Code generation or refactoring tasks
    Code,
}

/// Model tier for routing decisions
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ModelTier {
    /// Fast, cheap local model
    Fast,
    /// Balanced performance
    Balanced,
    /// Best reasoning capability
    Reasoner,
    /// Code-specialized model
    CodeGen,
}

/// The routing decision
#[derive(Debug, Clone)]
pub struct ModelSelection {
    pub tier: ModelTier,
    pub complexity: Complexity,
    pub reasoning: String,
}

/// Configuration for the model router
#[derive(Debug, Clone)]
pub struct RouterConfig {
    /// Model name for Fast tier (Ollama/local)
    pub fast_model: String,
    /// Model name for Balanced tier
    pub balanced_model: String,
    /// Model name for Reasoner tier
    pub reasoner_model: String,
    /// Model name for CodeGen tier
    pub codegen_model: String,
    /// Provider for each tier
    pub fast_provider: String,
    pub balanced_provider: String,
    pub reasoner_provider: String,
    pub codegen_provider: String,
}

impl Default for RouterConfig {
    fn default() -> Self {
        Self {
            fast_model: "codellama".into(),
            balanced_model: "gpt-4o-mini".into(),
            reasoner_model: "claude-sonnet-4-6".into(),
            codegen_model: "gpt-4o".into(),
            fast_provider: "ollama".into(),
            balanced_provider: "openai".into(),
            reasoner_provider: "anthropic".into(),
            codegen_provider: "openai".into(),
        }
    }
}

/// The model router: analyzes queries and selects the best model
pub struct ModelRouter {
    config: RouterConfig,
}

impl ModelRouter {
    pub fn new(config: RouterConfig) -> Self {
        Self { config }
    }

    /// Analyze a query and select the appropriate model
    pub fn route(&self, query: &str) -> ModelSelection {
        let complexity = self.analyze_complexity(query);
        let tier = self.select_tier(&complexity);

        ModelSelection {
            tier: tier.clone(),
            complexity,
            reasoning: self.explain(&tier),
        }
    }

    /// Analyze the complexity of a query using heuristics
    fn analyze_complexity(&self, query: &str) -> Complexity {
        let lower = query.to_lowercase();
        let len = query.len();

        // Code-related keywords
        let code_keywords = [
            "code", "implement", "function", "struct", "class", "refactor",
            "bug", "fix", "compile", "syntax", "api", "endpoint", "route",
            "component", "hook", "test", "import", "export", "async", "await",
            "fn ", "let ", "const ", "var ", "def ", "pub ", "mod ", "use ",
        ];
        let code_score = code_keywords.iter().filter(|k| lower.contains(*k)).count();

        // Deep reasoning keywords
        let reason_keywords = [
            "analyze", "design", "architect", "why", "explain deep", "complex",
            "tradeoff", "compare", "evaluate", "review", "assess", "strategy",
            "pattern", "principle", "best practice", "optimize performance",
        ];
        let reason_score = reason_keywords.iter().filter(|k| lower.contains(*k)).count();

        // Multi-step indicators
        let multistep_indicators = [
            "first", "then", "after", "finally", "step", "plan",
        ];
        let multistep_score = multistep_indicators.iter().filter(|k| lower.contains(*k)).count();

        // Length-based heuristics
        if len < 50 && code_score == 0 && reason_score == 0 {
            return Complexity::Simple;
        }

        if code_score >= 3 {
            return Complexity::Code;
        }

        if reason_score >= 3 || (reason_score >= 1 && len > 200) {
            return Complexity::Complex;
        }

        if code_score >= 1 || multistep_score >= 2 || len > 100 {
            return Complexity::Medium;
        }

        Complexity::Simple
    }

    /// Select the model tier based on complexity
    fn select_tier(&self, complexity: &Complexity) -> ModelTier {
        match complexity {
            Complexity::Simple => ModelTier::Fast,
            Complexity::Medium => ModelTier::Balanced,
            Complexity::Complex => ModelTier::Reasoner,
            Complexity::Code => ModelTier::CodeGen,
        }
    }

    /// Get the model name for a given tier
    pub fn model_for(&self, tier: &ModelTier) -> &str {
        match tier {
            ModelTier::Fast => &self.config.fast_model,
            ModelTier::Balanced => &self.config.balanced_model,
            ModelTier::Reasoner => &self.config.reasoner_model,
            ModelTier::CodeGen => &self.config.codegen_model,
        }
    }

    /// Get the provider for a given tier
    pub fn provider_for(&self, tier: &ModelTier) -> &str {
        match tier {
            ModelTier::Fast => &self.config.fast_provider,
            ModelTier::Balanced => &self.config.balanced_provider,
            ModelTier::Reasoner => &self.config.reasoner_provider,
            ModelTier::CodeGen => &self.config.codegen_provider,
        }
    }

    /// Explain the routing decision
    fn explain(&self, tier: &ModelTier) -> String {
        match tier {
            ModelTier::Fast => "Simple query → routing to fast local model for low-latency response.".into(),
            ModelTier::Balanced => "Moderate complexity → routing to balanced cloud model.".into(),
            ModelTier::Reasoner => "Complex reasoning required → routing to advanced reasoning model.".into(),
            ModelTier::CodeGen => "Code generation task → routing to code-specialized model.".into(),
        }
    }

    /// Auto-route and return the (provider, model) pair
    pub fn auto_route(&self, query: &str) -> (&str, &str) {
        let selection = self.route(query);
        (
            self.provider_for(&selection.tier),
            self.model_for(&selection.tier),
        )
    }
}

impl Default for ModelRouter {
    fn default() -> Self {
        Self::new(RouterConfig::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_query() {
        let router = ModelRouter::default();
        let result = router.route("hello");
        assert_eq!(result.complexity, Complexity::Simple);
        assert_eq!(result.tier, ModelTier::Fast);
    }

    #[test]
    fn test_code_query() {
        let router = ModelRouter::default();
        let result = router.route("implement a new function that reads files and parses JSON");
        assert_eq!(result.complexity, Complexity::Medium);
    }

    #[test]
    fn test_complex_query() {
        let router = ModelRouter::default();
        let result = router.route(
            "analyze the architecture of our distributed system and evaluate tradeoffs between event-driven and request-response patterns",
        );
        assert_eq!(result.complexity, Complexity::Complex);
    }

    #[test]
    fn test_code_query_detected() {
        let router = ModelRouter::default();
        let result = router.route("implement a new function to parse JSON and refactor the API endpoint to use async await");
        assert_eq!(result.complexity, Complexity::Code);
    }

    #[test]
    fn test_very_short_message_is_simple() {
        let router = ModelRouter::default();
        let result = router.route("hi");
        assert_eq!(result.complexity, Complexity::Simple);
        assert_eq!(result.tier, ModelTier::Fast);
    }

    #[test]
    fn test_medium_for_moderate_query() {
        let router = ModelRouter::default();
        let result = router.route("explain how the error handling works in this codebase and suggest improvements for the current implementation");
        assert_eq!(result.complexity, Complexity::Medium);
    }

    #[test]
    fn test_auto_route_returns_provider_and_model() {
        let router = ModelRouter::default();
        let (provider, model) = router.auto_route("hello");
        assert_eq!(provider, "ollama");
        assert_eq!(model, "codellama");
    }

    #[test]
    fn test_model_for_tiers() {
        let router = ModelRouter::default();
        assert_eq!(router.model_for(&ModelTier::Fast), "codellama");
        assert_eq!(router.model_for(&ModelTier::Balanced), "gpt-4o-mini");
        assert_eq!(router.model_for(&ModelTier::Reasoner), "claude-sonnet-4-6");
        assert_eq!(router.model_for(&ModelTier::CodeGen), "gpt-4o");
    }
}
