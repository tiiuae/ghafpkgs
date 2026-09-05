# SPDX-FileCopyrightText: 2026 TII (SSRC) and the Ghaf contributors
# SPDX-License-Identifier: Apache-2.0
{
  runCommand,
  imageTools,
  cryptsetupOffline,
  jq,
  lvm2,
  zstd,
}:
{
  plan =
    runCommand "ghaf-initialize-verity-lvm-plan"
      {
        nativeBuildInputs = [
          imageTools.lvm
          jq
          lvm2
          zstd
        ];
      }
      ''
        mkdir payload
        printf 'root\n' > root.raw
        printf 'verity\n' > verity.raw
        zstd root.raw -o payload/ghaf_root_1_deadbeef.raw.zst
        zstd verity.raw -o payload/ghaf_verity_1_deadbeef.raw.zst
        cat > payload/ghaf_1_deadbeef.manifest <<'EOF'
        {
          "manifest_version": 2,
          "root": { "file": "ghaf_root_1_deadbeef.raw.zst", "unpacked_size": 5 },
          "verity": { "file": "ghaf_verity_1_deadbeef.raw.zst", "unpacked_size": 7 }
        }
        EOF
        ln -s payload payload-link
        initialize() {
          ghaf-initialize-verity-lvm --update-dir payload-link \
            --root-size-mib 1 --verity-size-mib 1 "$@"
        }

        initialize \
          --create-inactive-slots --swap-size-mib 2 --persist-size-mib 3 \
          --print-plan > plan.json
        test "$(jq -r .lv_suffix plan.json)" = 1_deadbeef
        test "$(jq -r .minimum_pv_size_mib plan.json)" = 73
        ! initialize --device /dev/null 2> device.err
        grep -q '^Usage:' device.err

        truncate -s 96M first.img
        truncate -s 96M second.img
        for image in first.img second.img; do
          initialize --vg-name ghaf_test --image "$image"
        done
        mkdir -p stock-lvm/archive stock-lvm/backup
        export LVM_SYSTEM_DIR=$PWD/stock-lvm
        for image in first second; do
          pvck --driverloaded n --nolocking --config 'global { locking_type = 0 }' \
            --dump metadata "$image.img" > "$image-metadata.txt"
        done
        cmp first-metadata.txt second-metadata.txt
        cmp first.img second.img
        grep 'ghaf_test {' first-metadata.txt
        grep 'root_1_deadbeef {' first-metadata.txt
        grep 'verity_1_deadbeef {' first-metadata.txt

        # Exercise the shared bounded writer with locally formatted files too.
        truncate -s 256M complete.img
        initialize \
          --create-inactive-slots --swap-size-mib 4 --persist-size-mib 128 \
          --vg-name ghaf_complete --image complete.img
        pvck --driverloaded n --nolocking --config 'global { locking_type = 0 }' \
          --dump metadata complete.img > complete-metadata.txt
        for name in root_empty verity_empty swap persist; do
          grep "$name {" complete-metadata.txt
        done

        truncate -s $((1024 * 1024 + 1)) overlong-root.raw
        zstd --force overlong-root.raw -o payload/ghaf_root_1_deadbeef.raw.zst
        truncate -s 96M overlong.img
        ! initialize --vg-name ghaf_test --image overlong.img 2> overlong.err
        grep -q 'Payload for root_1_deadbeef exceeds its 1 MiB logical volume' \
          overlong.err
        test "$(stat -c%s overlong.img)" -eq $((96 * 1024 * 1024))

        # A decoder failure must be propagated, not hidden by its stdout pipe.
        printf 'not a zstd stream' > payload/ghaf_root_1_deadbeef.raw.zst
        truncate -s 96M corrupt.img
        ! initialize --vg-name ghaf_test --image corrupt.img 2> corrupt.err
        grep -q 'failed to decompress' corrupt.err
        touch "$out"
      '';
  roundtrip =
    runCommand "ghaf-wrap-luks-image-roundtrip"
      {
        nativeBuildInputs = [
          imageTools.luks
          cryptsetupOffline
          jq
        ];
      }
      ''
        # Exercise QEMU option escaping and the x86 empty bootstrap key too.
        mkdir 'with,comma'
        cd 'with,comma'
        for passphrase in test-passphrase ""; do
        truncate -s 64M plaintext.img
        # Written zero data is not a hole. Also cover mixed zero/nonzero sectors
        # and a distant extent to check that encryption uses absolute offsets.
        dd if=/dev/zero of=plaintext.img bs=4096 seek=4 count=1 conv=notrunc status=none
        dd if=/dev/zero of=plaintext.img bs=4096 seek=8192 count=2 conv=notrunc status=none
        printf 'middle' | dd of=plaintext.img bs=1 seek=$((32 * 1024 * 1024 + 512)) \
          conv=notrunc status=none
        dd if=plaintext.img of=expected-middle bs=4096 skip=8192 count=2 status=none
        printf 'ghaf-luks-start' | dd of=plaintext.img conv=notrunc status=none
        printf 'ghaf-luks-end' | dd of=plaintext.img bs=1 seek=$((64 * 1024 * 1024 - 13)) \
          conv=notrunc status=none
        printf '%s' "$passphrase" > key

        ghaf-wrap-luks-image \
          --image plaintext.img \
          --uuid 01234567-89ab-4cde-8fab-0123456789ab \
          --key-file key \
          --header-size-mib 32
        test "$(stat -c%s plaintext.img)" -eq $((96 * 1024 * 1024))
        test "$(du -B1 plaintext.img | cut -f1)" -lt $((40 * 1024 * 1024))
        # The cheap construction slot is gone; the retained key uses the
        # normal LUKS2 KDF, not its temporary 1000-iteration PBKDF2 setting.
        cryptsetup --disable-locks luksDump --dump-json-metadata plaintext.img | \
          jq -e '(.keyslots | length) == 1 and
            ([.keyslots[].kdf | .type == "argon2id" or
              (.type == "pbkdf2" and .iterations > 1000)] | all)'
        printf 'wrong-passphrase' > wrong-key
        ! cryptsetup --disable-locks open --test-passphrase \
          --key-file wrong-key plaintext.img

        mkdir -p /tmp/cryptsetup
        cryptsetup reencrypt --decrypt --force-offline-reencrypt \
          --batch-mode --key-file key \
          --header exported-header plaintext.img
        truncate -s 64M plaintext.img
        dd if=plaintext.img bs=4096 skip=4 count=1 status=none | cmp -n 4096 - /dev/zero
        dd if=plaintext.img bs=4096 skip=8192 count=2 status=none | cmp - expected-middle
        test "$(dd if=plaintext.img bs=1 count=15 status=none)" = ghaf-luks-start
        test "$(dd if=plaintext.img bs=1 skip=$((64 * 1024 * 1024 - 13)) \
          count=13 status=none)" = ghaf-luks-end
        # Decrypted sparse holes are unspecified, so start each case fresh.
        rm plaintext.img exported-header
        done
        touch "$out"
      '';
}
