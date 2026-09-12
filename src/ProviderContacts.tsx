import React from 'react';
export function ServiceDescription(){return <><p>GAIA — Global Auroral Image Aggregator — compiles low-resolution views from publicly available auroral cameras. Its goal is to provide a global overview of available cameras and easy access to the provider web pages offering high-resolution imagery.</p><p><strong>We do not provide high-resolution images or an archival image service.</strong> Playback here is a low-resolution overview, not an archive of original camera observations. Contact the principal investigators (PIs) or operators of the individual cameras for access to high-resolution archival images. Follow the camera links below or click imagery on the globe to visit the originating provider.</p></>}
const contacts=[
["Narsarsuaq","DTU Space and Tromsø Geophysical Observatory, UiT","Magnar Gullikstad Johnsen — responsible editor on the camera page; contact the operating institutions for data.","https://fox.phys.uit.no/ASC/NAQ.html"],
["Aðaldalshraun","NetNürds, North Iceland","Tim — camera website operator and contact.","https://www.netnurds.com/"],
  [
    "IRF Kiruna / KAGO",
    "Swedish Institute of Space Physics",
    "Urban Brändström — Head of Observatory and all-sky camera contact.",
    "https://www.irf.se/en/forskning/kago/firmamentkamera/"
  ],
  [
    "BACC Skibotn",
    "Tromsø Geophysical Observatory, UiT; Kjell Henriksen Observatory, UNIS",
    "Fred Sigernes — BACC collaborator; Magnar Gullikstad Johnsen — responsible editor and TGO contact.",
    "https://fox.phys.uit.no/ASC/BACC5.html"
  ],
  [
    "UCalgary TREx RGB",
    "University of Calgary Space Remote Sensing and participating observatories",
    "Emma Spanswick — instrument data contact.",
    "https://data.phys.ucalgary.ca/data/datasets/trex_rgb.html"
  ],
  [
    "UCalgary REGO",
    "University of Calgary Space Remote Sensing and participating observatories",
    "Emma Spanswick and Eric Donovan — instrument data contacts.",
    "https://data.phys.ucalgary.ca/data/datasets/rego.html"
  ],
  [
    "Other UCalgary instruments",
    "University of Calgary Space Remote Sensing and participating observatories",
    "Emma Spanswick and Eric Donovan — platform contacts; consult the instrument-specific page for data access and citation.",
    "https://data.phys.ucalgary.ca/"
  ],
  [
    "Norsk Meteornettverk",
    "Norsk Meteornettverk and volunteer station operators",
    "Steinar Midtskogen — network contact. Individual station ownership is not inferred from network membership.",
    "https://norskmeteornettverk.no/wordpress/?p=1329"
  ],
  [
    "THAAO, Thule / Pituffik",
    "ENEA, INGV and participating investigators",
    "Daniela Meloni — All-Sky Camera PI. Consult the PI before scientific publication, as requested by the provider.",
    "https://www.thuleatmos-it.it/dataaccess/allskycamera/index.php"
  ],
  [
    "Sodankylä all-sky cameras",
    "Sodankylä Geophysical Observatory, University of Oulu",
    "Request SGO data through the observatory; guest instruments have their own PIs.",
    "https://sgodata.sgo.fi/Data/Optical/allskyData.php"
  ],
  [
    "STARVISOR Night Sky Patrol",
    "Independent station operators and the STARVISOR network",
    "See each station link for its operator. Individual operator and PI names have not yet been independently verified. Camera links remain public; protected images require authorization.",
    "https://starvisor.net/"
  ],
  [
    "Hornsund",
    "Polish Polar Station Hornsund, Institute of Geophysics, Polish Academy of Sciences",
    "Use the station website to contact the operating institution; an individual camera PI has not been verified.",
    "https://hornsund.igf.edu.pl/index.php/en/cameras-2/?lang=en"
  ]
];
export function ProviderContacts(){return <section aria-label="Camera investigators and operators"><h2>Investigators &amp; operators</h2><p>Provider-confirmed roles and data-access links, checked September 2026. A network contact is not necessarily the PI or owner of every camera. Copyright and provider terms remain with the originating producers.</p>{contacts.map(([name,institution,role,url])=><details key={name}><summary>{name}</summary><p><strong>{institution}</strong><br/>{role}<br/><a href={url} target="_blank" rel="noreferrer">Provider information &amp; contact ↗</a></p></details>)}</section>}
