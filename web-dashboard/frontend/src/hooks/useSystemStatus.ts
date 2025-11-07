import { useState, useEffect } from 'react';
import { apiClient } from '../lib/api';

export interface SystemStatus {
  status: string;
  version: string;
  uptime_seconds: number;
  active_backups: number;
  total_backups: number;
  storage_used_bytes: number;
  system_health: 'Healthy' | 'Warning' | 'Critical' | 'Unknown';
}

export function useSystemStatus(refreshInterval: number = 30000) {
  const [status, setStatus] = useState<SystemStatus | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const fetchStatus = async () => {
    try {
      setError(null);
      const response = await apiClient.get<SystemStatus>('/status');
      setStatus(response);
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to fetch system status');
      console.error('Failed to fetch system status:', err);
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    // Fetch immediately
    fetchStatus();

    // Set up polling
    const interval = setInterval(fetchStatus, refreshInterval);

    return () => clearInterval(interval);
  }, [refreshInterval]);

  return {
    status,
    loading,
    error,
    refresh: fetchStatus,
  };
}