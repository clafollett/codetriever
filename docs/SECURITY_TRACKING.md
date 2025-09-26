# Security Issue Tracking

This document tracks known security advisories that we've acknowledged and are monitoring.

## Active Vulnerabilities (Allowlisted)

### RUSTSEC-2023-0071 - RSA Timing Sidechannel (Medium Severity)
- **Crate**: `rsa 0.9.8` (via sqlx-mysql dependency)
- **Issue**: Marvin Attack timing sidechannel vulnerability
- **Impact**: Potential RSA key recovery through timing analysis
- **Our Risk**: **NONE** - We don't use RSA keys in our application
- **Status**: Allowlisted in justfile
- **Action Plan**:
  - Monitor sqlx project for fixes
  - Consider alternatives if sqlx remains stagnant
  - Remove allowlist when vulnerability is patched

### RUSTSEC-2024-0436 - Unmaintained Paste Crate (Warning)
- **Crate**: `paste 1.0.15` (via agenterra-rmcp and candle-core)
- **Issue**: Crate is no longer maintained (archived Oct 2024)
- **Impact**: No security patches, potential future compatibility issues
- **Our Risk**: **LOW** - Proc-macro only, compile-time dependency
- **Status**: Allowlisted in justfile
- **Action Plan**:
  - Alternative exists: `pastey` crate (drop-in replacement)
  - Monitor agenterra-rmcp and candle-core for updates
  - Consider migration if dependencies don't update

### Monitoring Schedule
- [ ] Check monthly for sqlx security updates
- [ ] Review allowlist quarterly
- [ ] Consider migration if issue remains unfixed by Q2 2025

## Resolved Issues
None yet.

## Notes
- All allowlisted vulnerabilities should have clear justification
- Include impact assessment for our specific use case
- Set timelines for reassessment or migration