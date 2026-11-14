pub(in crate::cli) const CLONE_HELP: &str =
    "Runs `git clone` on the server's machine into a new folder inside FOLDER, named after the
repository as git names it, then opens that folder as a project, as `cornercase open` does, and
prints its path once git is done. A folder of that name that exists and is not empty is refused;
an empty one is used. A FOLDER with `..` in it is refused, and so is a repository named `.git`.
A repository on this machine can be given by its path, relative to the current folder. A clone
that fails leaves no folder behind, the folders it made for FOLDER included, and says what failed:
not a repository, access denied, or git's own reason. git never asks for a password: the server's
ssh keys and git credentials must already reach the repository. With --remote, FOLDER and a
repository's path are the other machine's (absolute, or starting with ~/), and without FOLDER the
clone goes in that machine's home folder. --boot says which run of the server the --view id is from.

--file copies a file of the repository here into the new clone once git is done, such as the
git-ignored .env a project needs to run, as the Clone on machine dialog does with the ones you
tick. This command reads it, on the machine it runs on also with --remote, and sends it with its
permissions. PATH goes from the root of the repository that --from is in (the current folder by
default). Refused by name before anything is cloned: a path that is absolute, goes up with `..` or
into `.git` (status 2); a file that is missing, a folder, a link or a file over 1 MiB (status 1),
and more than 200 files or 8 MiB in all. A file the clone could not take, such as one the
repository has, leaves the clone done and is not written: it is named on standard error, or under
\"unwritten\" with --json, and the command exits 3.

--list-ignored prints the git-ignored files in that folder, which the dialog offers, and clones
nothing: each from the repository's root with its size, and `too big to copy` over 1 MiB, then how
many more past the 200th. With --json: {\"root\", \"files\": [{\"path\", \"size\", \"mode\", \"too_big\"}],
\"left_out\"}. It reads this machine's folder, since the files are here even when the clone goes
elsewhere, so it needs no server, and --remote changes nothing for it, and it does not go with --boot. On Windows --file and
--list-ignored say they are not supported yet.

Examples:
  cornercase clone git@github.com:acme/shop.git ~/src
  cornercase clone https://github.com/acme/shop --focus
  cornercase --remote devbox clone git@github.com:acme/shop.git ~/src --json
  cornercase clone --list-ignored --from ~/code/shop --json
  cornercase --remote devbox clone git@github.com:acme/shop.git '~/src' --file .env --from ~/code/shop";
