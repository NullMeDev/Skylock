# Security

Skylock implements military-grade encryption and security best practices to protect your data.

## Encryption

### Client-Side Encryption

All data is encrypted on your local machine before being uploaded to cloud storage. Your encryption keys never leave your device.

- **Algorithm**: AES-256-GCM (Galois/Counter Mode)
- **Key Derivation**: Argon2id with configurable parameters
- **Key Size**: 256 bits
- **Authentication**: Authenticated encryption with additional data (AEAD)

### Transport Security

- **Protocol**: TLS 1.3
- **Hetzner WebDAV**: HTTPS with certificate validation
- **Hetzner SFTP**: SSH with Ed25519 key authentication

## Setup

### Generate Encryption Key

Generate a secure encryption key and add it to your configuration:

```bash
# Generate a random 256-bit key (base64 encoded)
openssl rand -base64 32
```

Add this to `~/.config/skylock-hybrid/config.toml`:

```toml
[hetzner]
encryption_key = "YOUR_GENERATED_KEY_HERE"
```

### SFTP Key Authentication (Recommended)

For enhanced security with SFTP:

1. Generate an Ed25519 SSH key:
```bash
ssh-keygen -t ed25519 -f ~/.ssh/id_ed25519_hetzner -C "hetzner-storagebox"
```

2. Upload public key to Hetzner:
```bash
cat ~/.ssh/id_ed25519_hetzner.pub
# Add this to your Storage Box authorized_keys via the web interface
```

3. Configure SFTP in config.toml:
```toml
[hetzner]
protocol = "sftp"
port = 23
```

## Security Best Practices

### Key Management

- **Store encryption keys securely**: Use a password manager or secure vault
- **Backup keys safely**: Without keys, encrypted data cannot be recovered
- **Rotate keys regularly**: Consider periodic key rotation
- **Never commit keys**: Ensure keys are not in version control

### Access Control

- **Use SSH keys**: Prefer key-based authentication over passwords
- **Limit permissions**: Use dedicated accounts with minimal permissions
- **Enable 2FA**: Where supported, enable two-factor authentication

### Data Protection

- **Verify backups**: Regularly test restore procedures
- **Monitor logs**: Check for unusual activity
- **Secure configuration files**: Restrict permissions on config files
  ```bash
  chmod 600 ~/.config/skylock-hybrid/config.toml
  ```

## Threat Model

Skylock protects against:

- **Storage provider compromise**: Data is encrypted before upload
- **Network interception**: TLS/SSH encrypts data in transit
- **Unauthorized access**: Strong encryption and authentication
- **Data tampering**: AEAD provides integrity verification

Skylock does NOT protect against:

- **Compromised client machine**: If your local system is compromised, keys may be exposed
- **Weak passwords**: Use strong, unique encryption keys
- **Lost keys**: Without keys, data cannot be recovered

## Security Audit

See [SECURITY_AUDIT.md](SECURITY_AUDIT.md) for details on the security audit performed before public release.

## Reporting Security Issues

If you discover a security vulnerability, please email null@nullme.lol with details. Do not open a public issue.

We will respond within 48 hours and work to address the issue promptly.

## Compliance

Skylock uses cryptographic libraries that are widely audited and comply with:

- FIPS 140-2 (when using appropriate backends)
- NIST recommendations for cryptographic algorithms
- Industry standard encryption practices
