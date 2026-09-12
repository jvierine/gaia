import React,{createContext,useContext,useEffect,useRef,useState} from 'react';
import {setPublicAudience} from './public-manifest';
type Session={client_id:string;nonce:string;email?:string|null;authorized:boolean};
type GoogleIdentity={accounts:{id:{initialize:(options:Record<string,unknown>)=>void;renderButton:(element:HTMLElement,options:Record<string,unknown>)=>void}}};
declare global {interface Window {google?:GoogleIdentity}}
let script:Promise<void>|undefined;
const AccountContext=createContext<React.ReactNode>(null);
export function AccountInfo(){return <>{useContext(AccountContext)}</>}
function loadGoogle(){return script??=new Promise<void>((resolve,reject)=>{const s=document.createElement('script');s.src='https://accounts.google.com/gsi/client';s.async=true;s.onload=()=>resolve();s.onerror=()=>{script=undefined;reject(Error('Google sign-in could not load'));};document.head.appendChild(s);});}
export default function GoogleAccess({children}:{children:React.ReactNode}){
 const bar=useRef<HTMLDivElement>(null);
 const [session,setSession]=useState<Session|null>(null),[error,setError]=useState(''),[ready,setReady]=useState(false),[generation,setGeneration]=useState(0);const button=useRef<HTMLDivElement>(null),current=useRef<boolean|undefined>(undefined);
 useEffect(()=>{const element=bar.current;if(!element)return;const observer=new ResizeObserver(()=>{if(element.isConnected)document.documentElement.style.setProperty('--gaia-account-height',element.getBoundingClientRect().height+'px')});observer.observe(element);return()=>{observer.disconnect();document.documentElement.style.removeProperty('--gaia-account-height')}},[session?.email]);
 const signedIn=useRef(false);
 const apply=(s:Session)=>{setPublicAudience(s.authorized,!!s.email);if(signedIn.current!==!!s.email){signedIn.current=!!s.email;setGeneration(v=>v+1)}if(current.current!==s.authorized){current.current=s.authorized;setGeneration(v=>v+1);}setSession(s);setReady(true)};
 const refresh=async()=>{const r=await fetch('/gaia/auth/session',{cache:'no-store'});if(!r.ok)throw Error('Sign-in is temporarily unavailable');apply(await r.json());};
 useEffect(()=>{void refresh().catch(e=>{setPublicAudience(false);setReady(true);setError(e.message)});const t=setInterval(()=>{void refresh().catch(()=>{setPublicAudience(false);if(current.current){current.current=false;setGeneration(v=>v+1);}setSession(null);setError('Sign-in is temporarily unavailable');})},60000);return()=>clearInterval(t)},[]);
 useEffect(()=>{if(!session||session.email||!button.current)return;let live=true;void loadGoogle().then(()=>{if(!live||!button.current)return;window.google!.accounts.id.initialize({client_id:session.client_id,nonce:session.nonce,auto_select:false,callback:async({credential}:{credential:string})=>{try{const r=await fetch('/gaia/auth/google',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({credential})});if(!r.ok)throw Error(await r.text());await refresh();setError('');}catch(e){setError(String(e));void refresh();}}});button.current.replaceChildren();window.google!.accounts.id.renderButton(button.current,{theme:'outline',size:'medium',text:'signin_with',type:'standard'});}).catch(e=>setError(e.message));return()=>{live=false}},[session?.nonce,session?.email]);
 const logout=async()=>{const r=await fetch('/gaia/auth/logout',{method:'POST'});if(!r.ok){setError('Could not sign out; please retry');return;}setPublicAudience(false);current.current=false;setGeneration(v=>v+1);await refresh();};
 const account=session?.email?<section aria-label="Google account"><h2>Your account</h2><p style={{overflowWrap:'anywhere'}}>{session.email}<br/>{session.authorized?'Starvisor access enabled':'No Starvisor access'}</p><button onClick={()=>void logout()}>Sign out</button>{error&&<p role="alert">{error}</p>}</section>:null;
 return <AccountContext.Provider value={account}>{!session?.email&&<div ref={bar} className="gaia-access" aria-label="Google account"><span>Starvisor imagery requires an authorized Google account.</span><div ref={button}/>{error&&<span role="alert">{error}</span>}</div>}{ready&&<React.Fragment key={generation}>{children}</React.Fragment>}</AccountContext.Provider>;
}
