import { createRoot } from 'react-dom/client';
import App from './App';
import './app.css';
import { applyLookClass } from './viewport/look';

// The look saved on this machine, before anything draws (light unless dark was chosen).
applyLookClass();

// No StrictMode: its double-mounted effects would create two GPU devices.
createRoot(document.getElementById('root')!).render(<App />);
