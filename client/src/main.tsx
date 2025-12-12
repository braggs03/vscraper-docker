import { useState } from "react";
import ReactDOM from "react-dom/client";
import { BrowserRouter, Route, Routes } from "react-router";
import App from "./App";
import { ThemeProvider } from "./components/theme-provider";
import Homepage from "./Homepage";



function Root() {
    const [hasSeenHomepage, setHasSeenHomepage] = useState(false);

    return (
        <ThemeProvider defaultTheme="light" storageKey="vite-ui-theme">
            <BrowserRouter>
                <Routes>
                    <Route path="/" element={<App hasSeenHomepage={hasSeenHomepage} />} />
                    <Route path="/starter" element={<Homepage onGetStarted={() => setHasSeenHomepage(true)} />} />
                </Routes>
            </BrowserRouter>
        </ThemeProvider>
    );
}

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(<Root />);
