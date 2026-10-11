use pumpkin_util::PermissionLvl;
use pumpkin_util::permission::{Permission, PermissionDefault, PermissionRegistry};
use pumpkin_util::text::{TextComponent, color::NamedColor};

use crate::command::argument_builder::{ArgumentBuilder, command};
use crate::command::context::command_context::CommandContext;
use crate::command::node::dispatcher::CommandDispatcher;
use crate::command::node::{CommandExecutor, CommandExecutorResult};
use crate::server::tick_timings::{self, MsptPercentiles, PhaseDurations, TickPhase};

const DESCRIPTION: &str = "Displays the server TPS and MSPT.";
const PERMISSION: &str = "pumpkin:command.tps";

struct TpsExecutor;

fn tps_color(tps: f64, max_tps: f64) -> NamedColor {
    if tps >= max_tps * 0.9 {
        NamedColor::Green
    } else if tps >= max_tps * 0.75 {
        NamedColor::Yellow
    } else {
        NamedColor::Red
    }
}

fn millis_text(nanos: i64) -> String {
    format!("{}ms", tick_timings::format_tick_millis(nanos))
}

fn summary_message(
    tps: f64,
    mspt: f64,
    max_tps: f64,
    percentiles: MsptPercentiles,
) -> TextComponent {
    let color = tps_color(tps, max_tps);
    TextComponent::text("TPS: ")
        .add_child(TextComponent::text(format!("{tps:.1}")).color_named(color))
        .add_child(TextComponent::text(" MSPT: "))
        .add_child(TextComponent::text(format!("{mspt:.2}ms")).color_named(color))
        .add_child(TextComponent::text(" p50: "))
        .add_child(TextComponent::text(millis_text(percentiles.p50_nanos)).color_named(color))
        .add_child(TextComponent::text(" p95: "))
        .add_child(TextComponent::text(millis_text(percentiles.p95_nanos)).color_named(color))
        .add_child(TextComponent::text(" p99: "))
        .add_child(TextComponent::text(millis_text(percentiles.p99_nanos)).color_named(color))
        .add_child(TextComponent::text(format!(
            " ({} ticks)",
            percentiles.samples
        )))
}

fn phase_message(phases: PhaseDurations) -> TextComponent {
    let mut line = String::from("Phases (last tick): ");
    for (index, phase) in TickPhase::ALL.iter().enumerate() {
        if index > 0 {
            line.push_str(", ");
        }
        line.push_str(phase.as_str());
        line.push_str(": ");
        line.push_str(&millis_text(phases.nanos(*phase)));
    }
    TextComponent::text(line).color_named(NamedColor::Gray)
}

impl CommandExecutor for TpsExecutor {
    fn execute(&self, context: &CommandContext) -> CommandExecutorResult {
        let server = context.source.server();
        let tps = server.get_tps().min(server.basic_config.tps as f64);
        let mspt = server.get_mspt();
        let max_tps = server.basic_config.tps as f64;

        context.source.send_message(summary_message(
            tps,
            mspt,
            max_tps,
            server.mspt_percentiles(),
        ));
        context
            .source
            .send_message(phase_message(server.latest_tick_phases()));

        Ok(tps as i32)
    }
}

pub fn register(dispatcher: &mut CommandDispatcher, registry: &PermissionRegistry) {
    registry.register_permission_or_panic(Permission::new(
        PERMISSION,
        DESCRIPTION,
        PermissionDefault::Op(PermissionLvl::Two),
    ));

    dispatcher.register(
        command("tps", DESCRIPTION)
            .requires(PERMISSION)
            .executes(TpsExecutor),
    );
}

#[cfg(test)]
mod tests {
    use super::{millis_text, phase_message, summary_message};
    use crate::server::tick_timings::{MsptPercentiles, PhaseDurations, TickPhase};

    #[test]
    fn summary_includes_percentiles() {
        let text = summary_message(
            20.0,
            1.25,
            20.0,
            MsptPercentiles {
                p50_nanos: 1_000_000,
                p95_nanos: 2_500_000,
                p99_nanos: 8_000_000,
                samples: 100,
            },
        )
        .get_text();

        assert!(text.contains("p50: 1.00ms"), "{text}");
        assert!(text.contains("p95: 2.50ms"), "{text}");
        assert!(text.contains("p99: 8.00ms"), "{text}");
        assert!(text.contains("(100 ticks)"), "{text}");
    }

    #[test]
    fn phase_that_did_not_run_prints_zero() {
        let text = phase_message(PhaseDurations::default()).get_text();
        for phase in TickPhase::ALL {
            let expected = format!("{}: {}", phase.as_str(), millis_text(0));
            assert!(text.contains(&expected), "{text} missing {expected}");
        }
        assert_eq!(millis_text(0), "0.00ms");
    }
}
