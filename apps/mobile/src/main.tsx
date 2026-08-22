import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import { MobileApp } from './MobileApp';
import './styles/mobile.css';

const rootElement = document.getElementById('root');
if (!rootElement) {
  throw new Error('Root element #root not found');
}

createRoot(rootElement).render(
  <StrictMode>
    <MobileApp />
  </StrictMode>,
);
