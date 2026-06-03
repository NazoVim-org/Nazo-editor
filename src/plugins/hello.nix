# ijevim Nix plugin example
# Evaluated via nix-instantiate --eval --json
# Returns config with optional repo_url for auto-install

{
  name = "hello-nix";
  version = "0.1.0";
  description = "A minimal Nix plugin example for ijevim";

  # Optional: auto-clone a GitHub repo on setup
  # repo_url = "https://github.com/user/ijevim-plugin";
}
