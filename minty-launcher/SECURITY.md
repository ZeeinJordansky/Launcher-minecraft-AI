# Security policy

Report security issues privately to the repository maintainers rather than publishing tokens, personal data, or exploit details in public issues.

Minty Launcher checks Mojang-published SHA-1 hashes for game files and Modrinth SHA-1/SHA-512 hashes for mod downloads. Microsoft refresh tokens are stored in Windows Credential Manager; access tokens stay in process memory.

Update releases carry an ECDSA/SHA-256 signature over the exact update-manifest bytes. The matching public key is embedded in the Native AOT executable. The launcher checks the signature before downloading an installer and verifies the installer SHA-256 before launching it. There is no unsigned fallback. The private key belongs only in the repository Actions secret `UPDATE_SIGNING_PRIVATE_KEY`; never commit it, put it in a release, or paste it into source files.

The launcher is not an anti-cheat and cannot guarantee detection of modified clients or malicious local software. Never send Microsoft tokens or credential-store exports in bug reports.
