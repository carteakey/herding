# Failures making a new worktree for work on herdr-core (Herdr 2), which keeps
# no worktrees of its own, so the checkout is made where Herdr 1 would.
# $complaint is Git's own message, in its words.

error-client-worktree-gave-up = The new worktree was abandoned: its run stopped or ran out of time
error-client-worktree-no-home = This computer has no home folder for new worktrees
error-client-worktree-no-repository-name = That folder's repository has no name to file a worktree under
error-client-worktree-refused = Git refused the new worktree: { $complaint }
error-client-worktree-start = Git could not be started
