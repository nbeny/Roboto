//! Noeud ROS 2 exposant le coeur du robot.
//!
//! Ce binaire est de la plomberie et rien d'autre. Il abonne, il publie, il traduit.
//! Aucune decision robotique n'est prise ici : l'autorite des commandes appartient a
//! `robot-core`, et la derniere borne sur la vitesse a `robot-safety`.
//!
//! # Interface
//!
//! | Direction | Nom | Type | Role |
//! |---|---|---|---|
//! | entree | `/cmd_vel` | `geometry_msgs/Twist` | teleoperation |
//! | entree | `/nav/cmd_vel` | `geometry_msgs/Twist` | pile de navigation |
//! | entree | `~/request_state` | `std_msgs/String` | transition d'etat par nom |
//! | sortie | `~/state` | `std_msgs/String` | etat operationnel |
//! | sortie | `~/cmd_vel_safe` | `geometry_msgs/Twist` | consigne apres securite |
//! | service | `~/emergency_stop` | `std_srvs/Trigger` | engage l'arret d'urgence |
//! | service | `~/clear_emergency_stop` | `std_srvs/Trigger` | leve l'arret d'urgence |
//! | service | `~/clear_safe_stop` | `std_srvs/Trigger` | quitte `SAFE_STOP` |
//!
//! Deux topics de commande distincts, et non un `/cmd_vel` partage : c'est le topic
//! d'arrivee qui determine la source, donc l'autorite. Les fusionner rendrait la table
//! d'habilitation du coeur inoperante.

mod bridge;
mod shared;

use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use geometry_msgs::msg::Twist;
// Trait d'extension : `create_basic_executor` est fourni par lui, pas par `Context`.
use rclrs::CreateBasicExecutor as _;
use robot_core::RobotConfig;
use robot_types::CommandSource;
use std_msgs::msg::String as StringMsg;

use crate::bridge::{names, parse_state, twist_from_velocity, velocity_from_twist};
use crate::shared::Shared;

/// Nom du noeud. Les topics en `~/` en heritent : `/robot_core/state`, etc.
const NODE_NAME: &str = "robot_core";

/// Periode de la boucle de controle, soit 50 Hz.
///
/// Confortablement sous le timeout de commande de 500 ms : le watchdog dispose de
/// 25 cycles pour se manifester.
const CONTROL_PERIOD: Duration = Duration::from_millis(20);

fn main() -> Result<(), Box<dyn std::error::Error>> {
    init_tracing();

    let context = rclrs::Context::default_from_env()?;
    let mut executor = context.create_basic_executor();
    let node = executor.create_node(NODE_NAME)?;

    let shared = Arc::new(Mutex::new(Shared::new(RobotConfig::default())?));

    // Horloge **monotone**, et non l'horloge du noeud.
    //
    // `node.get_clock()` rend le temps ROS, adosse a l'horloge murale hors simulation.
    // Or une horloge murale n'est pas monotone : mesure faite dans cette image, sur
    // 57,7 millions d'echantillons en 20 s, l'horloge murale a recule une fois de
    // 391 ms, la monotone jamais. Le coupable est la resynchronisation de la machine
    // virtuelle sur son hote — Docker Desktop et WSL2 y sont sujets, mais le probleme
    // n'a rien de specifique : NTP produit le meme effet sur une machine reelle.
    //
    // Un recul de cette ampleur fait basculer le robot en SAFE_STOP sans raison. Une
    // boucle de controle se cadence sur une horloge monotone, point.
    //
    // Le temps simule de Gazebo sera cable en Milestone 3, ou `use_sim_time` deviendra
    // pertinent : la source de temps sera alors le topic `/clock`, dont les reculs — une
    // relance du simulateur — sont de vraies discontinuites qu'il faut signaler.
    let clock = Arc::new(rclrs::Clock::steady());

    let state_publisher = node.create_publisher::<StringMsg>(names::STATE)?;
    let velocity_publisher = node.create_publisher::<Twist>(names::SAFE_CMD_VEL)?;

    let _teleop_subscription = {
        let shared = Arc::clone(&shared);
        let clock = Arc::clone(&clock);
        node.create_subscription(names::TELEOP_CMD_VEL, move |message: Twist| {
            on_velocity_command(&shared, &clock, &message, CommandSource::Teleoperation);
        })?
    };

    let _navigation_subscription = {
        let shared = Arc::clone(&shared);
        let clock = Arc::clone(&clock);
        node.create_subscription(names::NAVIGATION_CMD_VEL, move |message: Twist| {
            on_velocity_command(&shared, &clock, &message, CommandSource::Navigation);
        })?
    };

    let _state_request_subscription = {
        let shared = Arc::clone(&shared);
        let clock = Arc::clone(&clock);
        node.create_subscription(names::REQUEST_STATE, move |message: StringMsg| {
            on_state_request(&shared, &clock, &message.data);
        })?
    };

    let _emergency_stop_service = {
        let shared = Arc::clone(&shared);
        let clock = Arc::clone(&clock);
        node.create_service::<std_srvs::srv::Trigger, _>(
            names::EMERGENCY_STOP,
            move |_request: std_srvs::srv::Trigger_Request| {
                let mut guard = lock(&shared);
                let now = stamp_now(&mut guard, &clock);
                guard.engage_emergency_stop(now);

                trigger_response(true, "arret d'urgence engage")
            },
        )?
    };

    let _clear_emergency_stop_service = {
        let shared = Arc::clone(&shared);
        let clock = Arc::clone(&clock);
        node.create_service::<std_srvs::srv::Trigger, _>(
            names::CLEAR_EMERGENCY_STOP,
            move |_request: std_srvs::srv::Trigger_Request| {
                let mut guard = lock(&shared);
                let now = stamp_now(&mut guard, &clock);
                guard.clear_emergency_stop(now);

                trigger_response(
                    true,
                    "arret d'urgence leve ; le robot reste en SAFE_STOP jusqu'a clear_safe_stop",
                )
            },
        )?
    };

    let _clear_safe_stop_service = {
        let shared = Arc::clone(&shared);
        let clock = Arc::clone(&clock);
        node.create_service::<std_srvs::srv::Trigger, _>(
            names::CLEAR_SAFE_STOP,
            move |_request: std_srvs::srv::Trigger_Request| {
                let mut guard = lock(&shared);
                let now = stamp_now(&mut guard, &clock);

                match guard.clear_safe_stop(now) {
                    Ok(()) => trigger_response(true, "retour en IDLE"),
                    Err(error) => trigger_response(false, &error.to_string()),
                }
            },
        )?
    };

    // La boucle de controle vit dans son propre thread plutot que dans un callback
    // d'executeur : une boucle temps-reel souple ne doit pas dependre de l'ordonnancement
    // des callbacks ROS 2. Voir docs/architecture/0003-pas-d-async-dans-le-coeur.md.
    {
        let shared = Arc::clone(&shared);
        let clock = Arc::clone(&clock);
        thread::Builder::new()
            .name("control-loop".to_owned())
            .spawn(move || {
                let mut deadline = Instant::now();

                loop {
                    // Echeance absolue plutot que `sleep(PERIOD)` : sinon le temps de
                    // traitement de chaque cycle s'accumule en derive.
                    deadline += CONTROL_PERIOD;
                    if let Some(remaining) = deadline.checked_duration_since(Instant::now()) {
                        thread::sleep(remaining);
                    } else {
                        deadline = Instant::now();
                    }

                    let (state, velocity) = {
                        let mut guard = lock(&shared);
                        let now = stamp_now(&mut guard, &clock);
                        let output = guard.tick(now);
                        (output.state, output.velocity)
                    };

                    let mut state_message = StringMsg::default();
                    state_message.data = state.as_str().to_owned();

                    if let Err(error) = state_publisher.publish(&state_message) {
                        tracing::error!(event = "publish_failed", topic = names::STATE, %error);
                    }

                    if let Err(error) = velocity_publisher.publish(&twist_from_velocity(velocity)) {
                        tracing::error!(event = "publish_failed", topic = names::SAFE_CMD_VEL, %error);
                    }
                }
            })?;
    }

    tracing::info!(
        event = "node_started",
        node = NODE_NAME,
        period_ms = CONTROL_PERIOD.as_millis() as u64,
        "coeur du robot expose sur ROS 2"
    );

    // `spin` rend la main avec la liste des erreurs rencontrees par l'executeur. On les
    // journalise toutes, puis on remonte la premiere comme code de sortie du processus.
    let errors = executor.spin(rclrs::SpinOptions::default());

    for error in &errors {
        tracing::error!(event = "executor_error", "{error}");
    }

    match errors.into_iter().next() {
        Some(error) => Err(error.into()),
        None => Ok(()),
    }
}

/// Lit l'horloge ROS 2 et l'horodate, **sous le verrou deja detenu**.
///
/// L'atomicite des deux operations n'est pas un detail. Les lire separement autorise cet
/// entrelacement entre le thread de controle et un callback d'abonnement :
///
/// ```text
/// callback        : lit l'horloge -> t1
/// boucle controle : lit l'horloge -> t2 (t2 > t1)
/// boucle controle : prend le verrou, horodate t2
/// callback        : prend le verrou, horodate t1     <-- recul apparent
/// ```
///
/// [`robot_types::MonotonicClock`] detecte ce recul et force un `SAFE_STOP`, ce qui est
/// exactement son role — mais ici la perte de repere temporel serait imaginaire. Prendre
/// le verrou d'abord rend l'ordre des horodatages identique a l'ordre d'acquisition.
///
/// L'horloge vient du noeud et non du systeme : c'est ce qui permettra au meme binaire de
/// tourner sous Gazebo avec `use_sim_time`.
fn stamp_now(shared: &mut Shared, clock: &rclrs::Clock) -> robot_types::Monotonic {
    let nanos = u64::try_from(clock.now().nsec).unwrap_or(0);
    shared.stamp(nanos)
}

/// Verrouille l'etat partage.
///
/// En cas d'empoisonnement — un callback a panique en tenant le verrou — on echoue
/// bruyamment plutot que de poursuivre sur un etat dont on ne sait plus rien. Le filet de
/// securite est ailleurs, et il est materiel : le watchdog du microcontroleur coupe les
/// moteurs des que `cmd_vel_safe` cesse d'arriver, et l'arret d'urgence coupe la puissance.
fn lock(shared: &Mutex<Shared>) -> std::sync::MutexGuard<'_, Shared> {
    shared
        .lock()
        .expect("etat partage empoisonne : un callback a panique en tenant le verrou")
}

/// Soumet une consigne recue sur un topic de commande.
///
/// La commande est datee de son instant d'arrivee, faute de mieux : `geometry_msgs/Twist`
/// ne porte pas d'en-tete, donc aucun horodatage d'emission. Les controles de peremption
/// et d'horodatage futur du coeur ne peuvent donc rien attraper par ce chemin — ils
/// prendront leur sens en Milestone 4, sur la liaison serie du microcontroleur, ou les
/// messages sont horodates a la source.
fn on_velocity_command(
    shared: &Mutex<Shared>,
    clock: &rclrs::Clock,
    twist: &Twist,
    source: CommandSource,
) {
    let mut guard = lock(shared);
    let now = stamp_now(&mut guard, clock);

    if let Err(rejection) = guard.submit(velocity_from_twist(twist), source, now) {
        // Journalise sans bruit excessif : une commande refusee est un evenement
        // ordinaire quand l'etat ne s'y prete pas.
        tracing::debug!(
            event = "command_rejected",
            source = source.as_str(),
            reason = rejection.kind(),
            "{rejection}"
        );
    }
}

fn on_state_request(shared: &Mutex<Shared>, clock: &rclrs::Clock, raw: &str) {
    let Some(target) = parse_state(raw) else {
        tracing::warn!(event = "unknown_state", requested = raw, "etat inconnu");
        return;
    };

    let mut guard = lock(shared);
    let now = stamp_now(&mut guard, clock);

    match guard.request_state(target, now) {
        Ok(()) => tracing::info!(event = "state_requested", target = target.as_str()),
        Err(error) => tracing::warn!(event = "transition_refused", "{error}"),
    }
}

fn trigger_response(success: bool, message: &str) -> std_srvs::srv::Trigger_Response {
    std_srvs::srv::Trigger_Response {
        success,
        message: message.to_owned(),
    }
}

fn init_tracing() {
    use tracing_subscriber::EnvFilter;

    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::fmt().with_env_filter(filter).init();
}
