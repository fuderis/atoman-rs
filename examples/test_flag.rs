use atoman::Flag;

static STATUS: Flag = Flag::new(false);

fn main() {
    assert!(!STATUS.is_disabled());

    STATUS.enable();
    assert!(STATUS.is_enabled());

    STATUS.disable();
    assert!(STATUS.is_disabled());

    STATUS.set(true);
    assert!(STATUS.is_enabled());
}
