/*
 * comms_patch.c
 *
 * Patched variant of client/src/comms.c capabilities-version check.
 *
 * The upstream check hard-fails (PM3_EDEVNOTSUPP) when the device's
 * CAPABILITIES_VERSION does not match the client's compile-time constant.
 * That prevents Tauri from obtaining capabilities through a bundled
 * proxmark3.exe built against a different PM5 commit.
 *
 * This patch relaxes the fatal check to a WARNING and copies whatever fields
 * the device reports, zeroing unknown trailing fields so the rest of the
 * client can continue operating against a mismatched (newer or older) device.
 *
 * Apply this patch in `.build-env/pm3-src/` via scripts/sandbox/apply-patches.sh
 * before building the client with `make -j4 PLATFORM=PM5`.
 *
 * Intended insertion point: client/src/comms.c, the
 * `CAPABILITIES_VERSION` comparison block after the CMD_CAPABILITIES response.
 */

/* PATCH START: relaxed capabilities version check (was PM3_EDEVNOTSUPP hard fail) */
if ((resp.length != sizeof(g_pm3_capabilities)) ||
    (resp.data.asBytes[0] != CAPABILITIES_VERSION)) {
    PrintAndLogEx(WARNING, _YELLOW_("Capabilities version mismatch: device v%u, client v%u. Continuing anyway..."),
                  resp.data.asBytes[0], CAPABILITIES_VERSION);
    /* DO NOT RETURN PM3_EDEVNOTSUPP -- copy what we can and continue */
    size_t copy_len = MIN(resp.length, sizeof(g_pm3_capabilities));
    memcpy(&g_pm3_capabilities, resp.data.asBytes, copy_len);
    /* Zero out unknown fields */
    if (copy_len < sizeof(g_pm3_capabilities)) {
        memset(((uint8_t*)&g_pm3_capabilities) + copy_len, 0,
               sizeof(g_pm3_capabilities) - copy_len);
    }
}
/* PATCH END */
