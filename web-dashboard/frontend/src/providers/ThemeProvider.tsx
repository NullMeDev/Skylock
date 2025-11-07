import React, { createContext, useContext, useEffect, useState } from 'react';

type Theme = 'dark' | 'light' | 'system';

interface ThemeContextType {
  theme: Theme;
  effectiveTheme: 'dark' | 'light';
  setTheme: (theme: Theme) => void;
  toggleTheme: () => void;
}

const ThemeContext = createContext<ThemeContextType | undefined>(undefined);

interface ThemeProviderProps {
  children: React.ReactNode;
  defaultTheme?: Theme;
  storageKey?: string;
}

export function ThemeProvider({
  children,
  defaultTheme = 'system',
  storageKey = 'skylock-theme',
}: ThemeProviderProps) {
  const [theme, setTheme] = useState<Theme>(() => {
    if (typeof window !== 'undefined') {
      return (localStorage.getItem(storageKey) as Theme) || defaultTheme;
    }
    return defaultTheme;
  });

  const [effectiveTheme, setEffectiveTheme] = useState<'dark' | 'light'>('dark');

  useEffect(() => {
    const root = window.document.documentElement;
    
    const updateEffectiveTheme = () => {
      let newEffectiveTheme: 'dark' | 'light';
      
      if (theme === 'system') {
        newEffectiveTheme = window.matchMedia('(prefers-color-scheme: dark)').matches
          ? 'dark'
          : 'light';
      } else {
        newEffectiveTheme = theme;
      }
      
      setEffectiveTheme(newEffectiveTheme);
      
      root.classList.remove('light', 'dark');
      root.classList.add(newEffectiveTheme);
      
      // Set CSS custom properties for the theme
      if (newEffectiveTheme === 'dark') {
        root.style.setProperty('--background', '15 15 15'); // #0f0f0f
        root.style.setProperty('--foreground', '255 255 255'); // #ffffff
        root.style.setProperty('--surface', '26 26 26'); // #1a1a1a
        root.style.setProperty('--border', '45 45 45'); // #2d2d2d
        root.style.setProperty('--muted-foreground', '163 163 163'); // #a3a3a3
        root.style.setProperty('--primary', '59 130 246'); // #3b82f6
      } else {
        root.style.setProperty('--background', '255 255 255'); // #ffffff
        root.style.setProperty('--foreground', '31 41 55'); // #1f2937
        root.style.setProperty('--surface', '248 250 252'); // #f8fafc
        root.style.setProperty('--border', '241 245 249'); // #f1f5f9
        root.style.setProperty('--muted-foreground', '107 114 128'); // #6b7280
        root.style.setProperty('--primary', '59 130 246'); // #3b82f6
      }
    };

    updateEffectiveTheme();

    // Listen for system theme changes
    if (theme === 'system') {
      const mediaQuery = window.matchMedia('(prefers-color-scheme: dark)');
      const handleChange = () => updateEffectiveTheme();
      
      mediaQuery.addEventListener('change', handleChange);
      return () => mediaQuery.removeEventListener('change', handleChange);
    }
  }, [theme]);

  const value: ThemeContextType = {
    theme,
    effectiveTheme,
    setTheme: (newTheme: Theme) => {
      localStorage.setItem(storageKey, newTheme);
      setTheme(newTheme);
    },
    toggleTheme: () => {
      const newTheme = effectiveTheme === 'dark' ? 'light' : 'dark';
      localStorage.setItem(storageKey, newTheme);
      setTheme(newTheme);
    },
  };

  return (
    <ThemeContext.Provider value={value}>
      {children}
    </ThemeContext.Provider>
  );
}

export function useTheme() {
  const context = useContext(ThemeContext);
  if (context === undefined) {
    throw new Error('useTheme must be used within a ThemeProvider');
  }
  return context;
}