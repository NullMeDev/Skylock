import React, { createContext, useContext, useEffect, useRef, useState } from 'react';
import toast from 'react-hot-toast';

export interface WebSocketMessage {
  type: 'BackupProgress' | 'SystemMetrics' | 'BackupCompleted' | 'Alert' | 'LogEntry' | 'HealthUpdate';
  data: any;
}

interface WebSocketContextType {
  isConnected: boolean;
  lastMessage: WebSocketMessage | null;
  sendMessage: (message: any) => void;
  connectionStatus: 'connecting' | 'connected' | 'disconnected' | 'error';
}

const WebSocketContext = createContext<WebSocketContextType | undefined>(undefined);

interface WebSocketProviderProps {
  children: React.ReactNode;
  url?: string;
  reconnectInterval?: number;
  maxReconnectAttempts?: number;
}

export function WebSocketProvider({
  children,
  url = 'ws://localhost:8080/api/v1/ws',
  reconnectInterval = 5000,
  maxReconnectAttempts = 10,
}: WebSocketProviderProps) {
  const [isConnected, setIsConnected] = useState(false);
  const [lastMessage, setLastMessage] = useState<WebSocketMessage | null>(null);
  const [connectionStatus, setConnectionStatus] = useState<'connecting' | 'connected' | 'disconnected' | 'error'>('disconnected');
  
  const websocket = useRef<WebSocket | null>(null);
  const reconnectAttempts = useRef(0);
  const reconnectTimeout = useRef<NodeJS.Timeout | null>(null);

  const connect = () => {
    try {
      setConnectionStatus('connecting');
      websocket.current = new WebSocket(url);

      websocket.current.onopen = () => {
        console.log('WebSocket connected');
        setIsConnected(true);
        setConnectionStatus('connected');
        reconnectAttempts.current = 0;
        
        toast.success('Connected to Skylock server', {
          duration: 2000,
          id: 'websocket-connected',
        });
      };

      websocket.current.onmessage = (event) => {
        try {
          const message: WebSocketMessage = JSON.parse(event.data);
          setLastMessage(message);

          // Handle different message types
          switch (message.type) {
            case 'BackupCompleted':
              toast.success(
                `Backup completed: ${message.data.backup_id}`,
                { duration: 4000 }
              );
              break;
              
            case 'Alert':
              const alertLevel = message.data.level?.toLowerCase();
              const toastFn = alertLevel === 'error' || alertLevel === 'critical' 
                ? toast.error 
                : alertLevel === 'warning' 
                ? toast.error
                : toast;
              
              toastFn(message.data.message, { duration: 6000 });
              break;
              
            case 'BackupProgress':
              // Progress updates don't need notifications
              break;
              
            case 'SystemMetrics':
              // Metrics updates are handled by components
              break;
              
            case 'HealthUpdate':
              if (message.data.status === 'Critical') {
                toast.error(
                  `System health: ${message.data.component} is critical`,
                  { duration: 8000 }
                );
              }
              break;
              
            default:
              console.log('Unhandled WebSocket message:', message);
          }
        } catch (error) {
          console.error('Failed to parse WebSocket message:', error);
        }
      };

      websocket.current.onclose = (event) => {
        console.log('WebSocket closed:', event.code, event.reason);
        setIsConnected(false);
        setConnectionStatus('disconnected');
        
        // Only show disconnect toast if it wasn't intentional
        if (event.code !== 1000) {
          toast.error('Connection to Skylock server lost', {
            duration: 3000,
            id: 'websocket-disconnected',
          });
        }

        // Attempt to reconnect if not closed intentionally
        if (event.code !== 1000 && reconnectAttempts.current < maxReconnectAttempts) {
          reconnectAttempts.current++;
          console.log(`Attempting to reconnect... (${reconnectAttempts.current}/${maxReconnectAttempts})`);
          
          reconnectTimeout.current = setTimeout(() => {
            connect();
          }, reconnectInterval);
        } else if (reconnectAttempts.current >= maxReconnectAttempts) {
          setConnectionStatus('error');
          toast.error(
            'Failed to reconnect to Skylock server. Please refresh the page.',
            { duration: 10000 }
          );
        }
      };

      websocket.current.onerror = (error) => {
        console.error('WebSocket error:', error);
        setConnectionStatus('error');
        toast.error('WebSocket connection error', { duration: 3000 });
      };

    } catch (error) {
      console.error('Failed to create WebSocket connection:', error);
      setConnectionStatus('error');
    }
  };

  const disconnect = () => {
    if (reconnectTimeout.current) {
      clearTimeout(reconnectTimeout.current);
      reconnectTimeout.current = null;
    }
    
    if (websocket.current) {
      websocket.current.close(1000, 'Intentional disconnect');
      websocket.current = null;
    }
    
    setIsConnected(false);
    setConnectionStatus('disconnected');
  };

  const sendMessage = (message: any) => {
    if (websocket.current?.readyState === WebSocket.OPEN) {
      websocket.current.send(JSON.stringify(message));
    } else {
      console.warn('WebSocket is not connected. Message not sent:', message);
      toast.error('Not connected to server. Message not sent.');
    }
  };

  useEffect(() => {
    connect();

    // Cleanup on unmount
    return () => {
      disconnect();
    };
  }, [url]);

  // Handle page visibility changes to manage connection
  useEffect(() => {
    const handleVisibilityChange = () => {
      if (document.visibilityState === 'visible' && !isConnected) {
        // Reconnect when tab becomes visible and we're disconnected
        connect();
      } else if (document.visibilityState === 'hidden') {
        // Optionally pause reconnection attempts when tab is hidden
        if (reconnectTimeout.current) {
          clearTimeout(reconnectTimeout.current);
          reconnectTimeout.current = null;
        }
      }
    };

    document.addEventListener('visibilitychange', handleVisibilityChange);
    return () => document.removeEventListener('visibilitychange', handleVisibilityChange);
  }, [isConnected]);

  const value: WebSocketContextType = {
    isConnected,
    lastMessage,
    sendMessage,
    connectionStatus,
  };

  return (
    <WebSocketContext.Provider value={value}>
      {children}
    </WebSocketContext.Provider>
  );
}

export function useWebSocket() {
  const context = useContext(WebSocketContext);
  if (context === undefined) {
    throw new Error('useWebSocket must be used within a WebSocketProvider');
  }
  return context;
}