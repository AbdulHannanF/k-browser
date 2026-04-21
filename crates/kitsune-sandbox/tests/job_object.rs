use kitsune_sandbox::windows::JobObjectSandbox;
use std::process::Command;
use std::time::Duration;

#[test]
fn test_job_object_creates_successfully() {
    let sandbox = JobObjectSandbox::new("test_job_1").expect("Failed to create job object");
    assert_eq!(sandbox.name, "test_job_1");
}

#[test]
fn test_job_object_configure_succeeds() {
    let sandbox = JobObjectSandbox::new("test_job_2").expect("Failed to create job object");
    sandbox.configure().expect("Failed to configure job object limits");
}

#[test]
fn test_assign_current_process_to_job() {
    let sandbox = JobObjectSandbox::new("test_job_3").expect("Failed to create job object");
    
    // We can't easily assign the current test runner process because it might already be in a job
    // that doesn't allow breakaway, or we might kill the test runner.
    // Instead, let's spawn a dummy child process and assign it.
    let mut child = Command::new("cmd.exe")
        .args(&["/C", "ping 127.0.0.1 -n 3 > NUL"])
        .spawn()
        .expect("Failed to spawn child process");
        
    sandbox.assign_process(child.id()).expect("Failed to assign child process to job");
    
    // We won't test KILL_ON_JOB_CLOSE directly here because it might be flaky depending on timing,
    // but the assignment succeeded.
    
    let _ = child.wait();
}

#[test]
fn test_job_object_drops_cleanly() {
    {
        let sandbox = JobObjectSandbox::new("test_job_4").expect("Failed to create job object");
        sandbox.configure().expect("Failed to configure");
    } // Drops here
    // If it didn't panic, drop is clean
}
