import { render } from 'preact';
import '@solana/wallet-adapter-react-ui/styles.css';
import './style.css';
import { App } from './app.jsx';

render(<App />, document.getElementById('app'));
