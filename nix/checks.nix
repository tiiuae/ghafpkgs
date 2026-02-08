# SPDX-FileCopyrightText: 2022-2026 TII (SSRC) and the Ghaf contributors
# SPDX-License-Identifier: Apache-2.0
{ inputs, ... }:
{
  imports = [ inputs.git-hooks-nix.flakeModule ];
  perSystem =
    {
      config,
      pkgs,
      self',
      lib,
      ...
    }:
    {
      checks = {
        pre-commit-check = config.pre-commit.devShell;

        # C++ static analysis with cppcheck
        # Note: Currently only fails on errors, not warnings/style
        # TODO: Enable --error-exitcode=1 for warnings after fixing existing issues
        cpp-static-analysis =
          pkgs.runCommand "cpp-static-analysis"
            {
              nativeBuildInputs = [ pkgs.cppcheck ];
              src = ../packages/cpp;
            }
            ''
              echo "Running cppcheck on C++ source files..."
              # Run cppcheck and capture output (warnings are informational for now)
              cppcheck \
                --enable=warning,style,performance,portability \
                --error-exitcode=1 \
                --suppress=missingIncludeSystem \
                --suppress=nullPointerOutOfMemory \
                --suppress=invalidPrintfArgType_sint \
                --suppress=constParameterReference \
                --suppress=useStlAlgorithm \
                --suppress=uselessAssignmentPtrArg \
                --suppress=normalCheckLevelMaxBranches \
                --inline-suppr \
                -q \
                "$src"
              echo "cppcheck passed successfully"
              touch $out
            '';
      }
      // (
        let
          # Filter out function attributes like 'override' and 'overrideDerivation'
          isPackage =
            name: _value:
            !(lib.elem name [
              "override"
              "overrideDerivation"
            ]);
          packageAttrs = lib.filterAttrs isPackage self'.packages;
        in
        lib.mapAttrs' (n: lib.nameValuePair "package-${n}") packageAttrs
      )
      # NixOS VM integration test for network packet forwarder (x86_64-linux only)
      # TODO: Enable after fixing test assertion in packet_gen_tests.nix
      # The test fails with: "Number of allowed mdns packets in internalvm must be lower than max expected val"
      # To run manually: nix build .#checks.x86_64-linux.nw-packet-forwarder-integration
      # // lib.optionalAttrs (pkgs.stdenv.hostPlatform.system == "x86_64-linux") {
      #   nw-packet-forwarder-integration = pkgs.testers.nixosTest (
      #     import ../packages/rust/ghaf-nw-packet-forwarder/test/integration/packet_gen_tests.nix {
      #       inherit pkgs;
      #       inherit (inputs) crane;
      #     }
      #   );
      # }
      ;

      pre-commit = {
        settings = {
          hooks = {
            treefmt = {
              enable = true;
              package = config.treefmt.build.wrapper;
              stages = [ "pre-push" ];
            };
            reuse = {
              enable = true;
              package = pkgs.reuse;
              stages = [ "pre-push" ];
            };
            end-of-file-fixer = {
              enable = true;
              stages = [ "pre-push" ];
            };
          };
        };
      };
    };
}
