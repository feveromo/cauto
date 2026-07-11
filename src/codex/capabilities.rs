use crate::error::AppError;
use crate::routing::{
    CapabilitySource, Downgrade, FastMode, LaunchMode, ModelFamily, ReasoningLevel, RoutePreset,
};

use super::catalog::{ModelCapability, ModelCatalog};

#[derive(Clone, Debug)]
pub struct PresetRequest {
    pub model_id: Option<String>,
    pub family: ModelFamily,
    pub effort: ReasoningLevel,
    pub mode: LaunchMode,
    pub fast_mode: FastMode,
    pub explicit_model: bool,
    pub explicit_effort: bool,
    pub allow_downgrade: bool,
}

#[derive(Clone, Debug)]
pub struct ResolvedPreset {
    pub preset: RoutePreset,
    pub downgrade: Option<Downgrade>,
}

fn effort_rank(value: &str) -> Option<u8> {
    match value.to_ascii_lowercase().as_str() {
        "minimal" => Some(0),
        "low" => Some(1),
        "medium" => Some(2),
        "high" => Some(3),
        "xhigh" => Some(4),
        "max" => Some(5),
        "ultra" => Some(6),
        _ => None,
    }
}

fn strongest_supported_at_or_below(
    model: &ModelCapability,
    requested: ReasoningLevel,
) -> Option<String> {
    let requested_rank = effort_rank(requested.native_name())?;
    model
        .supported_reasoning_efforts
        .iter()
        .filter_map(|effort| effort_rank(effort).map(|rank| (rank, effort)))
        .filter(|(rank, _)| *rank <= requested_rank)
        .max_by_key(|(rank, _)| *rank)
        .map(|(_, effort)| effort.clone())
}

fn unlisted_explicit(
    request: &PresetRequest,
    model_id: String,
) -> Result<ResolvedPreset, AppError> {
    if matches!(request.effort, ReasoningLevel::Max | ReasoningLevel::Ultra) {
        return Err(AppError::PresetUnavailable(format!(
            "{} / {} is not proven by the installed catalog",
            model_id,
            request.effort.display_name()
        )));
    }
    Ok(ResolvedPreset {
        preset: RoutePreset {
            model_family: ModelFamily::from_model_id(&model_id),
            model_id,
            display_level: request.effort,
            native_effort: Some(request.effort.native_name().into()),
            collaboration_mode: None,
            service_tier: None,
            required_features: Vec::new(),
            interactive_supported: true,
            exec_supported: true,
            source: CapabilitySource::Fallback,
            fallback: None,
        },
        downgrade: None,
    })
}

/// Resolves abstract family/effort choices against proven installed capabilities.
pub fn resolve_preset(
    catalog: &ModelCatalog,
    request: &PresetRequest,
) -> Result<ResolvedPreset, AppError> {
    let model = if let Some(model_id) = &request.model_id {
        match catalog.find(model_id) {
            Some(model) => model,
            None if request.explicit_model => {
                return unlisted_explicit(request, model_id.clone());
            }
            None => {
                return Err(AppError::PresetUnavailable(format!(
                    "model {model_id:?} is not in the installed catalog"
                )));
            }
        }
    } else {
        catalog.first_family(&request.family).ok_or_else(|| {
            AppError::PresetUnavailable(format!(
                "no visible {} model is present in the installed catalog",
                request.family
            ))
        })?
    };
    let supported_for_mode = match request.mode {
        LaunchMode::Interactive => model.interactive_supported,
        LaunchMode::Exec => model.exec_supported,
    };
    if !supported_for_mode {
        return Err(AppError::PresetUnavailable(format!(
            "{} is not selectable through {:?}",
            model.id, request.mode
        )));
    }

    let (native_effort, downgrade) = if model.supports_effort(request.effort.native_name()) {
        (request.effort.native_name().to_owned(), None)
    } else {
        let fallback = strongest_supported_at_or_below(model, request.effort).ok_or_else(|| {
            AppError::PresetUnavailable(format!(
                "{} exposes none of the supported effort values at or below {}",
                model.id,
                request.effort.display_name()
            ))
        })?;
        if request.explicit_effort && !request.allow_downgrade {
            return Err(AppError::ExplicitDowngradeRefused(format!(
                "{} does not expose literal effort {}",
                model.id,
                request.effort.native_name()
            )));
        }
        if !request.explicit_effort && !request.allow_downgrade {
            return Err(AppError::PresetUnavailable(format!(
                "automatic route {} / {} is unavailable and automatic downgrade is disabled",
                model.id,
                request.effort.display_name()
            )));
        }
        (
            fallback.clone(),
            Some(Downgrade {
                requested: format!("{} / {}", model.id, request.effort.display_name()),
                selected: format!("{} / {fallback}", model.id),
                reason: "installed model catalog does not expose the requested effort".into(),
            }),
        )
    };
    let selected_level = native_effort
        .parse::<ReasoningLevel>()
        .map_err(AppError::PresetUnavailable)?;
    let service_tier = match request.fast_mode {
        FastMode::Inherit => None,
        FastMode::Fast => model
            .service_tiers
            .iter()
            .find(|tier| {
                tier.name.eq_ignore_ascii_case("fast") || tier.id.eq_ignore_ascii_case("priority")
            })
            .map(|tier| tier.id.clone())
            .or_else(|| {
                model
                    .additional_speed_tiers
                    .iter()
                    .any(|tier| tier.eq_ignore_ascii_case("fast"))
                    .then(|| "priority".into())
            })
            .ok_or_else(|| {
                AppError::PresetUnavailable(format!(
                    "{} does not advertise a Fast service tier",
                    model.id
                ))
            })
            .map(Some)?,
        // Native Codex uses `default` for Standard. Flex is a separate
        // cost/latency tier and must never be presented as Fast-off.
        FastMode::NoFast => Some("default".into()),
    };
    Ok(ResolvedPreset {
        preset: RoutePreset {
            model_id: model.id.clone(),
            model_family: model.family.clone(),
            display_level: selected_level,
            native_effort: Some(native_effort),
            collaboration_mode: None,
            service_tier,
            required_features: if selected_level == ReasoningLevel::Ultra {
                vec!["multi_agent".into()]
            } else {
                Vec::new()
            },
            interactive_supported: model.interactive_supported,
            exec_supported: model.exec_supported,
            source: catalog.source.clone(),
            fallback: None,
        },
        downgrade,
    })
}
