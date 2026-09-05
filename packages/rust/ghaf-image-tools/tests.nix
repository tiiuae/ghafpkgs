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
        reject() { if "$@"; then echo "Unexpected success: $*" >&2; exit 1; fi; }
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
        touch payload/unrelated.manifest
        initialize() {
          ghaf-initialize-verity-lvm --manifest payload-link/ghaf_1_deadbeef.manifest \
            --root-size-mib 1 --verity-size-mib 1 "$@"
        }

        initialize \
          --create-inactive-slots --swap-size-mib 2 --persist-size-mib 3 \
          --print-plan > plan.json
        test "$(jq -r .lv_suffix plan.json)" = 1_deadbeef
        test "$(jq -r .minimum_pv_size_mib plan.json)" = 73
        reject initialize --device /dev/null 2> device.err
        grep -q '^Usage:' device.err
        reject initialize --root-size-mib 0 --print-plan
        for option in --vg-name=other --update-dir=payload; do
          reject initialize "$option" --print-plan
        done

        truncate -s 96M first.img
        truncate -s 96M second.img
        for image in first.img second.img; do
          initialize --image "$image"
        done
        mkdir -p stock-lvm/archive stock-lvm/backup
        export LVM_SYSTEM_DIR=$PWD/stock-lvm
        for image in first second; do
          pvck --driverloaded n --nolocking --config 'global { locking_type = 0 }' \
            --dump headers "$image.img" > "$image-headers.txt"
          pvck --driverloaded n --nolocking --config 'global { locking_type = 0 }' \
            --dump metadata --file "$PWD/$image-metadata.txt" "$image.img"
          # Exercise stock LVM's VG importer, not just the binary header reader.
          vgcfgrestore --driverloaded n --nolocking --config 'global { locking_type = 0 }' \
            --list --file "$PWD/$image-metadata.txt" pool
        done
        cmp first-metadata.txt second-metadata.txt
        cmp first-headers.txt second-headers.txt
        cmp first.img second.img
        grep -q 'label_header.crc 0xcfb9d1e1' first-headers.txt
        grep -q 'pv_header_extension.flags 1' first-headers.txt
        grep -q 'pe_start = 10240' first-metadata.txt
        grep -q 'pe_count = 22' first-metadata.txt
        dd if=first.img bs=1 skip=$((5 * 1024 * 1024)) count=5 status=none | cmp - root.raw
        dd if=first.img bs=1 skip=$((9 * 1024 * 1024)) count=7 status=none | cmp - verity.raw
        grep 'pool {' first-metadata.txt
        grep 'root_1_deadbeef {' first-metadata.txt
        grep 'verity_1_deadbeef {' first-metadata.txt

        # Exercise the shared bounded writer with locally formatted files too.
        truncate -s 256M complete.img
        initialize \
          --create-inactive-slots --swap-size-mib 4 --persist-size-mib 128 \
          --image complete.img
        pvck --driverloaded n --nolocking --config 'global { locking_type = 0 }' \
          --dump metadata complete.img > complete-metadata.txt
        for name in root_empty verity_empty swap persist; do
          grep "$name {" complete-metadata.txt
        done

        # Stock pvck must reject damage to each independently checksummed area.
        for offset in 600 4500 4700; do
          cp --sparse=always first.img damaged.img
          printf '\377' | dd of=damaged.img bs=1 seek="$offset" conv=notrunc status=none
          reject pvck --driverloaded n --nolocking --config 'global { locking_type = 0 }' \
            --dump headers damaged.img
        done

        truncate -s $((1024 * 1024 + 1)) overlong-root.raw
        zstd --force overlong-root.raw -o payload/ghaf_root_1_deadbeef.raw.zst
        truncate -s 96M overlong.img
        reject initialize --image overlong.img 2> overlong.err
        grep -q 'Payload for root_1_deadbeef exceeds its 1 MiB logical volume' \
          overlong.err
        test "$(stat -c%s overlong.img)" -eq $((96 * 1024 * 1024))

        # Both short and long payloads that fit the LV must match the manifest.
        for size in 4 6; do
          truncate -s "$size" mismatch.raw
          zstd --force mismatch.raw -o payload/ghaf_root_1_deadbeef.raw.zst
          truncate -s 96M mismatch.img
          reject initialize --image mismatch.img 2> mismatch.err
          grep -q "expected 5 unpacked bytes, got $size" mismatch.err
        done

        # A decoder failure must be propagated, not hidden by its stdout pipe.
        printf 'not a zstd stream' > payload/ghaf_root_1_deadbeef.raw.zst
        truncate -s 96M corrupt.img
        reject initialize --image corrupt.img 2> corrupt.err
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
          --key-file key
        test "$(stat -c%s plaintext.img)" -eq $((96 * 1024 * 1024))
        test "$(du -B1 plaintext.img | cut -f1)" -lt $((40 * 1024 * 1024))
        # The cheap construction slot is gone; the retained key uses the
        # normal LUKS2 KDF, not its temporary 1000-iteration PBKDF2 setting.
        cryptsetup --disable-locks luksDump --dump-json-metadata plaintext.img | \
          jq -e '(.keyslots | length) == 1 and
            ([.keyslots[].kdf | .type == "argon2id" or
              (.type == "pbkdf2" and .iterations > 1000)] | all)'
        printf 'wrong-passphrase' > wrong-key
        if cryptsetup --disable-locks open --test-passphrase \
          --key-file wrong-key plaintext.img; then exit 1; fi

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
