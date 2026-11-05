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
clone goes in that machine's home folder.

Examples:
  cornercase clone git@github.com:acme/shop.git ~/src
  cornercase clone https://github.com/acme/shop --focus
  cornercase --remote devbox clone git@github.com:acme/shop.git ~/src --json";
