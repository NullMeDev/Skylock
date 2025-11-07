import React from 'react';
import { BrowserRouter as Router, Routes, Route, Navigate } from 'react-router-dom';
import { Toaster } from 'react-hot-toast';

// Layout Components
import { Layout } from './components/layout/Layout';

// Page Components
import { Dashboard } from './pages/Dashboard';
import { BackupsPage } from './pages/Backups';
import { RestorePage } from './pages/Restore';
import { MonitoringPage } from './pages/Monitoring';
import { SettingsPage } from './pages/Settings';

// Providers
import { ThemeProvider } from './providers/ThemeProvider';
import { WebSocketProvider } from './providers/WebSocketProvider';

// Global CSS
import './index.css';

function App() {
  return (
    <ThemeProvider>
      <WebSocketProvider>
        <Router>
          <div className="min-h-screen bg-background text-foreground">
            {/* Global Toast Notifications */}
            <Toaster
              position="top-right"
              toastOptions={{
                duration: 4000,
                className: 'bg-surface border border-border text-foreground shadow-lg',
                success: {
                  iconTheme: {
                    primary: '#10b981',
                    secondary: '#ffffff',
                  },
                },
                error: {
                  iconTheme: {
                    primary: '#ef4444',
                    secondary: '#ffffff',
                  },
                },
              }}
            />

            {/* Main Application Routes */}
            <Routes>
              <Route path="/" element={<Layout />}>
                {/* Dashboard Home */}
                <Route index element={<Dashboard />} />
                
                {/* Backup Management */}
                <Route path="backups" element={<BackupsPage />} />
                
                {/* File Restore */}
                <Route path="restore" element={<RestorePage />} />
                
                {/* System Monitoring */}
                <Route path="monitor" element={<MonitoringPage />} />
                
                {/* Settings */}
                <Route path="settings" element={<SettingsPage />} />
                
                {/* Catch-all redirect */}
                <Route path="*" element={<Navigate to="/" replace />} />
              </Route>
            </Routes>
          </div>
        </Router>
      </WebSocketProvider>
    </ThemeProvider>
  );
}

export default App;