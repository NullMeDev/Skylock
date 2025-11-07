use skylock_core::{Result, SkylockError, StorageErrorType};
use ssh2::Session;
use std::{
    path::{Path, PathBuf},
    io::{Read, Write},
    net::TcpStream,
};
use tokio::io::{AsyncWriteExt, AsyncReadExt};
use tracing::{info, debug};

pub struct SftpClient {
    session: Session,
    sftp: ssh2::Sftp,
}

impl SftpClient {
    pub fn connect(
        host: &str,
        port: u16,
        username: &str,
        password: &str
    ) -> Result<Self> {
        let tcp = TcpStream::connect(format!("{}:{}", host, port))
            .map_err(|e| SkylockError::Storage(StorageErrorType::ConnectionFailed(e.to_string())))?;

        let mut session = Session::new()
            .map_err(|e| SkylockError::Storage(StorageErrorType::ConnectionFailed(e.to_string())))?;

        session.set_tcp_stream(tcp);
        session.handshake()
            .map_err(|_| SkylockError::Storage(StorageErrorType::StorageBoxUnavailable))?;

        session.userauth_password(username, password)
            .map_err(|_| SkylockError::Storage(StorageErrorType::AuthenticationFailed))?;

        let sftp = session.sftp()
            .map_err(|_| SkylockError::Storage(StorageErrorType::StorageBoxUnavailable))?;

        Ok(Self { session, sftp })
    }

    pub async fn upload_file(&self, local_path: &Path, remote_path: &Path) -> Result<u64> {
        info!("Uploading file via SFTP: {} -> {}", local_path.display(), remote_path.display());

        let mut local_file = tokio::fs::File::open(local_path).await
            .map_err(|_| SkylockError::Storage(StorageErrorType::FileNotFound))?;

        // Create parent directories if they don't exist
        if let Some(parent) = remote_path.parent() {
            self.create_dir_all(parent).await
                .map_err(|_| SkylockError::Storage(StorageErrorType::AccessDenied))?;
        }

        let mut remote_file = self.sftp.create(remote_path)
            .map_err(|_| SkylockError::Storage(StorageErrorType::AccessDenied))?;

        let mut buffer = vec![0; 32768];
        let mut total_bytes = 0;

        loop {
            let n = local_file.read(&mut buffer).await
                .map_err(|e| SkylockError::Storage(StorageErrorType::IOError(e.to_string())))?;
            if n == 0 {
                break; // EOF
            }
            remote_file.write_all(&buffer[..n])
                .map_err(|e| SkylockError::Storage(StorageErrorType::IOError(e.to_string())))?;
            total_bytes += n as u64;
            debug!("Uploaded {} bytes", total_bytes);
        }

        Ok(total_bytes)
    }

    pub async fn download_file(&self, remote_path: &Path, local_path: &Path) -> Result<u64> {
        info!("Downloading file via SFTP: {} -> {}", remote_path.display(), local_path.display());

        // Create parent directories if they don't exist
        if let Some(parent) = local_path.parent() {
            tokio::fs::create_dir_all(parent).await
                .map_err(|_| SkylockError::Storage(StorageErrorType::AccessDenied))?;
        }

        let mut remote_file = self.sftp.open(remote_path)
            .map_err(|_| SkylockError::Storage(StorageErrorType::FileNotFound))?;

        let mut local_file = tokio::fs::File::create(local_path).await
            .map_err(|_| SkylockError::Storage(StorageErrorType::AccessDenied))?;

        let mut buffer = vec![0; 32768];
        let mut total_bytes = 0;

        loop {
            let n = remote_file.read(&mut buffer)
                .map_err(|e| SkylockError::Storage(StorageErrorType::IOError(e.to_string())))?;
            if n == 0 {
                break; // EOF
            }
            local_file.write_all(&buffer[..n]).await
                .map_err(|e| SkylockError::Storage(StorageErrorType::IOError(e.to_string())))?;
            total_bytes += n as u64;
            debug!("Downloaded {} bytes", total_bytes);
        }

        Ok(total_bytes)
    }

    pub fn delete_file(&self, remote_path: &Path) -> Result<()> {
        info!("Deleting file via SFTP: {}", remote_path.display());

        self.sftp.unlink(remote_path)
            .map_err(|e| SkylockError::Storage(StorageErrorType::IOError(e.to_string())))?;

        Ok(())
    }

    pub fn list_dir(&self, remote_path: &Path) -> Result<Vec<PathBuf>> {
        let entries = self.sftp.readdir(remote_path)
            .map_err(|e| SkylockError::Storage(StorageErrorType::IOError(e.to_string())))?;

        Ok(entries.into_iter()
            .map(|(path, _)| path)
            .collect())
    }

    async fn create_dir_all(&self, path: &Path) -> Result<()> {
        let mut current = PathBuf::new();
        for component in path.components() {
            current.push(component);
            match self.sftp.mkdir(&current, 0o755) {
                Ok(_) => debug!("Created directory: {}", current.display()),
                Err(e) if e.message().contains("already exists") => {
                    // Directory already exists, continue
                }
                Err(_) => return Err(SkylockError::Storage(StorageErrorType::AccessDenied)),
            }
        }
        Ok(())
    }
}
