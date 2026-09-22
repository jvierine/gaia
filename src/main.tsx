import React from 'react';
import { createRoot } from 'react-dom/client';
import Home from '../app/page';
import PublicViewer from './PublicViewer';
import '../app/globals.css';
import {archiveMode} from './public-manifest';
import EventStudy from './EventStudy';

const eventStudy=/\/gaia\/events\/\d{8}(?:\/|$)/.test(location.pathname);
createRoot(document.getElementById('root')!).render(<React.StrictMode>{eventStudy?<EventStudy/>:import.meta.env.VITE_GAIA_PUBLIC==='1'||archiveMode||new URLSearchParams(location.search).has('archive')?<PublicViewer/>:<Home />}</React.StrictMode>);
