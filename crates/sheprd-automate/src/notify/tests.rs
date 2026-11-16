use super::*;

#[test]
fn the_text_goes_as_arguments_not_as_script() {
    let title = "nightly\" & do shell script \"touch /tmp/x";
    let shown = command(title, "-n body");
    assert_eq!(shown.is_none(), cfg!(windows));
    let Some((_, args)) = shown else {
        return;
    };
    // Title and body are whole arguments of their own, after the script.
    assert_eq!(&args[args.len() - 2..], [title, "-n body"]);
    assert!(!args[..args.len() - 2].iter().any(|arg| arg.contains(title)));
}
