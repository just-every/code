### Ensure that tests pass

* `nix flake check`, or `cargo make ci`.
* I must manually approve CI runs for new PRs to prevent github-action attacks.

### Manually test the demos

* Run the `demo` and `async` examples, verify that everything works (resizing, cycling images...).
* There are no VM tests in place yet, hopefully someday we will have a terminal and OS test matrix.
* I will do a manual verification before merging, but please do not rely on me.

### Small PRs

* Open separate PRs for separate issues.
* It's okay for a PR to depend on another.
* Can merge the other PRs if minor issues are causing long discussions; faster development.
* One commit per PR:
    * Squash / amend changes.
    * Except for special cases when it is deemed really necessary for some reason.

### No merge commits

* Merge commits are generally not useful or relevant for *reviewing*.
* Use rebase instead.
* Better history.

### Code of Conduct

By participating, you agree to follow our [Code of Conduct](CODE_OF_CONDUCT.md).
