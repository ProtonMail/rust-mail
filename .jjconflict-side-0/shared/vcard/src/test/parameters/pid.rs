use crate::parameters::pid::Pid;
use crate::parameters::pid::is_pid_param;

#[test]
fn pid_struct() {
    assert!(Pid::new_validated(&["1.2".to_owned(), "3.4".to_owned()]).is_ok());
    // Invalid pid value (pid-value = 1*DIGIT ["." 1*DIGIT])
    assert!(Pid::new_validated(&["foo".to_owned()]).is_err());
}

#[test]
fn pid_param() {
    assert!(is_pid_param(&["123".to_owned()]));
    assert!(is_pid_param(&["123.456".to_owned()]));
    assert!(is_pid_param(&["123.456".to_owned(), "789.012".to_owned()]));
    assert!(!is_pid_param(&[String::new()]));
    assert!(!is_pid_param(&["foo".to_owned()]));
    assert!(!is_pid_param(&["123.0".to_owned()]));
    assert!(!is_pid_param(&[]));
}
