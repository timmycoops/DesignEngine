import { createRoot } from 'react-dom/client';
import App from './App';
import './app.css';

// No StrictMode: its double-mounted effects would create two GPU devices.
createRoot(document.getElementById('root')!).render(<App />);
