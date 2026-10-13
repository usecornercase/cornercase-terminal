use super::*;

#[test]
fn a_server_without_restart_when_idle_is_called_too_old_to_wait() {
    let refused = Error::Control(control::unknown_command("restart-when-idle"));

    assert_eq!(too_old_to_wait(refused).to_string(), TOO_OLD_TO_WAIT);
}
