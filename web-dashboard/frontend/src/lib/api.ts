import axios from 'axios';
import toast from 'react-hot-toast';

// API configuration
const API_BASE_URL = import.meta.env.VITE_API_BASE_URL || 'http://localhost:8080/api/v1';

// Create axios instance
const axiosInstance = axios.create({
  baseURL: API_BASE_URL,
  timeout: 10000, // 10 seconds
  headers: {
    'Content-Type': 'application/json',
  },
});

// Request interceptor
axiosInstance.interceptors.request.use(
  (config) => {
    // Add auth token if available
    const token = localStorage.getItem('auth_token');
    if (token) {
      config.headers.Authorization = `Bearer ${token}`;
    }

    // Add request timestamp for debugging
    config.metadata = { startTime: new Date() };
    
    return config;
  },
  (error) => {
    return Promise.reject(error);
  }
);

// Response interceptor
axiosInstance.interceptors.response.use(
  (response) => {
    // Log request duration in development
    if (import.meta.env.DEV && response.config.metadata) {
      const duration = new Date().getTime() - response.config.metadata.startTime.getTime();
      console.log(`API Request: ${response.config.method?.toUpperCase()} ${response.config.url} - ${duration}ms`);
    }

    // Extract data from the API response wrapper
    if (response.data && response.data.success !== undefined) {
      if (response.data.success) {
        return response.data.data;
      } else {
        throw new Error(response.data.error || 'API request failed');
      }
    }

    // Return data directly if no wrapper
    return response.data;
  },
  (error) => {
    console.error('API Error:', error);

    // Handle different error types
    if (error.code === 'ECONNABORTED') {
      toast.error('Request timeout. Please try again.');
    } else if (error.response) {
      // Server responded with error status
      const status = error.response.status;
      const message = error.response.data?.error || error.response.data?.message || 'Unknown error';
      
      switch (status) {
        case 401:
          toast.error('Authentication required. Please log in.');
          // Redirect to login if needed
          break;
        case 403:
          toast.error('Access denied. Insufficient permissions.');
          break;
        case 404:
          toast.error('Resource not found.');
          break;
        case 429:
          toast.error('Too many requests. Please wait a moment.');
          break;
        case 500:
          toast.error('Server error. Please try again later.');
          break;
        default:
          toast.error(message);
      }
      
      return Promise.reject(new Error(message));
    } else if (error.request) {
      // Network error
      toast.error('Network error. Please check your connection.');
      return Promise.reject(new Error('Network error'));
    } else {
      // Other errors
      toast.error('An unexpected error occurred.');
      return Promise.reject(error);
    }
  }
);

// API client methods
export const apiClient = {
  get: <T = any>(url: string, config?: any): Promise<T> =>
    axiosInstance.get(url, config),

  post: <T = any>(url: string, data?: any, config?: any): Promise<T> =>
    axiosInstance.post(url, data, config),

  put: <T = any>(url: string, data?: any, config?: any): Promise<T> =>
    axiosInstance.put(url, data, config),

  delete: <T = any>(url: string, config?: any): Promise<T> =>
    axiosInstance.delete(url, config),

  patch: <T = any>(url: string, data?: any, config?: any): Promise<T> =>
    axiosInstance.patch(url, data, config),
};

// Specific API endpoints
export const api = {
  // System endpoints
  getSystemStatus: () => apiClient.get('/status'),
  getSystemMetrics: () => apiClient.get('/metrics'),
  getSystemHealth: () => apiClient.get('/health'),
  getSystemLogs: (params?: any) => apiClient.get('/logs', { params }),

  // Backup endpoints
  getBackups: (params?: any) => apiClient.get('/backups', { params }),
  createBackup: (data: any) => apiClient.post('/backups', data),
  getBackup: (id: string) => apiClient.get(`/backups/${id}`),
  deleteBackup: (id: string) => apiClient.delete(`/backups/${id}`),
  getBackupFiles: (id: string) => apiClient.get(`/backups/${id}/files`),

  // Restore endpoints
  startRestore: (data: any) => apiClient.post('/restore', data),
  getRestoreStatus: (id: string) => apiClient.get(`/restore/${id}/status`),

  // Configuration endpoints
  getConfiguration: () => apiClient.get('/config'),
  updateConfiguration: (data: any) => apiClient.put('/config', data),
};

export default apiClient;