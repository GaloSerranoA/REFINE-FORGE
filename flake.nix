{
  description = "refineforge — Lean 4 proof engineering and refinement-bundle framework";

  # ─── Inputs (all pinned via flake.lock once `nix flake lock` is run) ───
  inputs = {
    nixpkgs.url      = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url  = "github:numtide/flake-utils";

    # Lean 4 toolchain pinned by content hash via the upstream
    # `lean4-nix` flake. Reads `lean/lean-toolchain` directly so a
    # Lean version bump in that file picks up here too. See
    # https://github.com/lenianiva/lean4-nix
    lean4-nix = {
      url = "github:lenianiva/lean4-nix";
      inputs.nixpkgs.follows = "nixpkgs";
      inputs.flake-utils.follows = "flake-utils";
    };

    # Rust toolchain. `rust-overlay` gives us pinned-by-channel rustc;
    # `crane` provides the build-then-cache pattern for Cargo workspaces.
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    crane = {
      url = "github:ipetkov/crane";
    };
  };

  outputs = { self, nixpkgs, flake-utils, lean4-nix, rust-overlay, crane }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        # ─── Pkgs with overlays ────────────────────────────────────────
        pkgs = import nixpkgs {
          inherit system;
          overlays = [
            rust-overlay.overlays.default
            # readToolchainFile reads `lean/lean-toolchain` and gives
            # us pkgs.lean-all, pkgs.lean, pkgs.lake at the pinned
            # version. The toolchain file format must be
            # `leanprover/lean4:vX.Y.Z`, which ours is.
            (lean4-nix.readToolchainFile ./lean/lean-toolchain)
          ];
        };

        # ─── Toolchains ────────────────────────────────────────────────
        rustToolchain = pkgs.rust-bin.stable.latest.default;
        craneLib = (crane.mkLib pkgs).overrideToolchain rustToolchain;

        # ─── Filtered source: keep cargo workspace + lean for bundle
        #     derivations + claims + docs/refinement for bundle export ─
        src = pkgs.lib.cleanSourceWith {
          src = ./.;
          filter = path: type:
            let
              baseName = baseNameOf (toString path);
              relPath  = pkgs.lib.removePrefix (toString ./. + "/") (toString path);
            in
              # Keep cargo sources (crane needs them)
              (craneLib.filterCargoSources path type)
              # AND keep lean/, claims/, templates/, docs/refinement/
              # for the bundle derivations. The repo-level support
              # paths are used by Cargo integration tests that run
              # inside the Nix source tree.
              || (pkgs.lib.hasPrefix "lean/" relPath
                  && !(pkgs.lib.hasPrefix "lean/.lake" relPath))
              || (pkgs.lib.hasPrefix "claims/" relPath)
              || (pkgs.lib.hasPrefix "templates/" relPath)
              || (pkgs.lib.hasPrefix "docs/refinement/" relPath)
              || (pkgs.lib.hasPrefix ".github/workflows/" relPath)
              || (pkgs.lib.hasPrefix "scripts/ci/" relPath)
              || (pkgs.lib.hasPrefix "release/" relPath)
              || (pkgs.lib.hasPrefix "kernels/" relPath)
              || (pkgs.lib.hasPrefix "training/" relPath)
              || (pkgs.lib.hasPrefix "containers/" relPath)
              || (pkgs.lib.elem relPath [ ".gitignore" "flake.nix" ]);
        };

        # Cargo dep artifacts cached separately so source-only changes
        # don't rebuild the world.
        cargoArtifacts = craneLib.buildDepsOnly {
          inherit src;
          pname = "refineforge-deps";
          version = "0.1.0";
        };

        # ─── Build each binary ─────────────────────────────────────────
        refine = craneLib.buildPackage {
          inherit src cargoArtifacts;
          pname = "refine";
          version = "0.1.0";
          cargoExtraArgs = "--bin refine --locked";
          # cargo nextest unit tests run as part of `nix flake check`
          # via the cargoTest stanza below, not here.
          doCheck = false;
          buildInputs = with pkgs; [
            # rustls-tls in refineforge-strategies needs no system deps;
            # listed empty for now. If TLS issues appear at build time,
            # add openssl + pkg-config and switch reqwest features to
            # `native-tls`.
          ];
        };

        refine-eval = craneLib.buildPackage {
          inherit src cargoArtifacts;
          pname = "refine-eval";
          version = "0.1.0";
          cargoExtraArgs = "--bin refine-eval --locked";
          doCheck = false;
        };

        # ─── Bundle-export derivation ──────────────────────────────────
        # Builds a verifiable bundle hermetically: copies the project
        # to a writable temp tree, runs `lake build`, then invokes
        # `refine bundle export <claim_id>`, then extracts the bundle.
        #
        # The whole project must be in `src` (we don't filter for this
        # path) — the bundle exporter needs claims/, lean/, etc.
        #
        # If `lake build` requires network (e.g. to fetch Mathlib),
        # this derivation will FAIL in Nix's sandboxed build because
        # network access is denied. Our EXAMPLE-* claims do NOT use
        # Mathlib so this should work; a Mathlib-using claim would
        # need `__noChroot = true` (impure) or a pre-built Mathlib
        # pkg in pkgs.
        bundleFor = claim_id: pkgs.stdenv.mkDerivation {
          name = "refineforge-bundle-${claim_id}";
          inherit src;
          nativeBuildInputs = [ refine pkgs.lean-all ];

          # Lake writes its build cache into lean/.lake/ ; needs HOME.
          buildPhase = ''
            runHook preBuild
            export HOME=$(mktemp -d)
            (cd lean && lake build)
            mkdir -p artifacts
            ${refine}/bin/refine bundle export ${claim_id}
            runHook postBuild
          '';

          installPhase = ''
            runHook preInstall
            mkdir -p $out
            cp -r artifacts/${claim_id}/* $out/
            runHook postInstall
          '';
        };

        # ─── Checks (`nix flake check` runs these) ─────────────────────
        cargoTest = craneLib.cargoTest {
          inherit src cargoArtifacts;
          pname = "refineforge-tests";
          version = "0.1.0";
          cargoTestExtraArgs = "--workspace";
        };

        cargoFmt = craneLib.cargoFmt {
          inherit src;
          pname = "refineforge-fmt";
          version = "0.1.0";
        };

        cargoClippy = craneLib.cargoClippy {
          inherit src cargoArtifacts;
          pname = "refineforge-clippy";
          version = "0.1.0";
          cargoClippyExtraArgs = "--workspace -- -D warnings";
        };
      in {
        packages = {
          inherit refine refine-eval;
          default = refine;

          # Hermetic bundle builds for the two tutorial claims.
          # `nix build .#bundle-EXAMPLE-002` reproduces a sealed
          # bundle bit-for-bit (modulo created_at; see
          # docs/reproducible-build.md §2).
          bundle-EXAMPLE-001 = bundleFor "EXAMPLE-001";
          bundle-EXAMPLE-002 = bundleFor "EXAMPLE-002";
        };

        # `nix flake check` runs all of these.
        checks = {
          inherit cargoTest cargoFmt cargoClippy;
          inherit (self.packages.${system}) refine refine-eval;
        };

        # `nix develop` — everything a contributor needs to work on
        # refineforge locally.
        devShells.default = pkgs.mkShell {
          packages = with pkgs; [
            rustToolchain
            cargo-nextest
            lean-all                  # lake + lean at the pinned version
            cosign                    # for signing / verifying bundles
            git
            jq                        # for poking at bundle manifests
            # The eval harness uses Python for the report-inspection
            # snippets in CHANGELOG; include for convenience.
            python3
          ];

          shellHook = ''
            echo "refineforge dev shell — see ARCHITECTURE.md for the three sections."
            echo "  rustc:  $(rustc --version)"
            echo "  cargo:  $(cargo --version)"
            echo "  lean:   $(lean --version)"
            echo "  cosign: $(cosign version --short 2>/dev/null || echo unknown)"
          '';
        };

        # `nix fmt` runs nixpkgs-fmt on the flake itself.
        formatter = pkgs.nixpkgs-fmt;
      });
}
