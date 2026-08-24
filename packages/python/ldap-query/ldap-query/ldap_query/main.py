# SPDX-FileCopyrightText: 2022-2026 TII (SSRC) and the Ghaf contributors
# SPDX-License-Identifier: Apache-2.0
import argparse
import ssl
import sys

from ldap3 import (
    ALL,
    AUTO_BIND_NO_TLS,
    AUTO_BIND_TLS_BEFORE_BIND,
    GSSAPI,
    SASL,
    Connection,
    Server,
    Tls,
)


def main():
    """
    Connects to AD, queries for users, and prints their details.
    """
    # Set up argument parser to read inputs from the command line
    parser = argparse.ArgumentParser(
        description="Query Active Directory for users using GSSAPI.",
        formatter_class=argparse.RawTextHelpFormatter,
    )
    parser.add_argument(
        "--server",
        required=True,
        help=(
            "Hostname of the Active Directory server, optionally prefixed with\n"
            "'ldaps://' for implicit TLS. A bare hostname (or 'ldap://') keeps\n"
            "port 389 and negotiates StartTLS before binding."
        ),
    )
    parser.add_argument(
        "--ca-certs-file",
        default=None,
        help=(
            "PEM bundle used to verify the server certificate.\n"
            "Defaults to the system trust store."
        ),
    )
    parser.add_argument(
        "--base-dn",
        required=True,
        help="Base DN for the LDAP search (e.g., 'dc=ad,dc=example,dc=com').",
    )
    parser.add_argument(
        "--min-uid", default="1", help="Minimum UID for the search range (default: 1)."
    )
    parser.add_argument(
        "--max-uid",
        default="999999",
        help="Maximum UID for the search range (default: 999999).",
    )
    args = parser.parse_args()

    # Define the LDAP filter and the attributes to retrieve using the parsed arguments
    ldap_filter = (
        f"(&(objectClass=user)(uidNumber>={args.min_uid})(uidNumber<={args.max_uid}))"
    )
    attributes_to_fetch = ["sAMAccountName", "displayName", "uidNumber", "gidNumber"]

    # Split off the URL scheme so the transport is chosen explicitly rather
    # than inferred.
    scheme, separator, host = args.server.strip().rpartition("://")
    if not separator:
        scheme = "ldap"
    scheme = scheme.lower()
    if scheme not in ("ldap", "ldaps"):
        print(
            f"Error: unsupported scheme '{scheme}://' for --server; "
            "expected ldap:// or ldaps://.",
            file=sys.stderr,
        )
        sys.exit(1)
    if not host:
        print("Error: --server does not name a host.", file=sys.stderr)
        sys.exit(1)

    # Everything this tool prints -- account names, uidNumber, gidNumber -- is
    # consumed downstream for local account provisioning, so the channel must be
    # confidential and tamper-proof. ldap3's GSSAPI SASL implementation
    # negotiates NO_SECURITY_LAYER and so supplies neither; TLS with a verified
    # certificate is the only thing standing between an on-path attacker and
    # forged uid/gid values. Certificate validation is mandatory here: ldap3's
    # default Tls object uses CERT_NONE, which would accept any certificate.
    tls = Tls(
        validate=ssl.CERT_REQUIRED,
        version=ssl.PROTOCOL_TLS_CLIENT,
        ca_certs_file=args.ca_certs_file,
    )

    # Define the server and create a connection object using SASL with GSSAPI for Kerberos authentication
    use_ssl = scheme == "ldaps"
    server = Server(host, use_ssl=use_ssl, tls=tls, get_info=ALL)
    conn = Connection(
        server,
        authentication=SASL,
        sasl_mechanism=GSSAPI,
        # ldaps:// is already wrapped in TLS; otherwise StartTLS is negotiated
        # before the bind. Both fail closed -- there is no plaintext fallback.
        auto_bind=AUTO_BIND_NO_TLS if use_ssl else AUTO_BIND_TLS_BEFORE_BIND,
        read_only=True,
    )

    # Search the LDAP directory and print matches to stdout
    try:
        search_successful = conn.search(
            search_base=args.base_dn,
            search_filter=ldap_filter,
            attributes=attributes_to_fetch,
        )

        if not search_successful:
            print(f"Error: LDAP search failed. {conn.result}", file=sys.stderr)
            sys.exit(1)

        # Process and print the results
        if not conn.entries:
            print(
                f"Info: No users found in the UID range {args.min_uid}-{args.max_uid}.",
                file=sys.stderr,
            )
            return

        for entry in conn.entries:
            user_data = [
                entry.sAMAccountName.value if "sAMAccountName" in entry else "N/A",
                entry.displayName.value if "displayName" in entry else "N/A",
                entry.uidNumber.value if "uidNumber" in entry else "N/A",
                entry.gidNumber.value if "gidNumber" in entry else "N/A",
            ]
            print("|".join(map(str, user_data)))

    except Exception as e:  # noqa: BLE001 - top-level CLI boundary: report and exit 1
        print(f"An error occurred: {e}", file=sys.stderr)
        sys.exit(1)

    finally:
        conn.unbind()
