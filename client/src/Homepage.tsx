import { useEffect, useState } from "react";
import { useNavigate } from "react-router";
import "./App.css";
import { useTheme } from "./components/theme-provider";
import { Button } from "./components/ui/button";
import { Checkbox } from "./components/ui/checkbox";
import { Label } from "./components/ui/label";
import { useQuery } from "@tanstack/react-query";

let api = import.meta.env.VITE_API_URL;

const updatePreference = (preference: boolean) => {
    fetch(new URL(`config/homepage/${preference}`, api), {
        method: "post"
    })
}

export default function Homepage({ onGetStarted }: { onGetStarted: () => void }) {
    const { theme } = useTheme();
    const navigate = useNavigate();
    const [preference, setPreference] = useState(false);

    const { isPending, data } = useQuery({
        queryKey: ['config'],
        queryFn: () =>
            fetch(new URL("/api/config", api)).then((res) =>
                res.json(),
        
            ),
    });

    useEffect(() => {
        data && setPreference(data.skip_homepage)
    }, [data])

    if (isPending) return 'Loading...'

    return (
        <main className="flex flex-col items-center justify-center text-center min-h-screen">
            <h1 className="mb-5 font-sans text-3xl">Welcome to</h1>
            <div className="w-80">
                {
                    theme ?
                        <img src={"/vscraper-dark.svg"} className="block dark:hidden w-full h-auto" alt="vscraper dark" />
                        :
                        <img src={"/vscraper-light.svg"} className="block dark:hidden w-full h-auto" alt="vscraper dark" />
                }
            </div>
            <h1 className="m-5 font-sans text-3xl">A Simple Tool for Youtube DL's Weak Spots.</h1>
            <div className="flex flex-row">
                <Button className="mr-1">
                    <a href="https://github.com/braggs03/vscraper" target="_blank">
                        About
                    </a>
                </Button>
                <Button variant="outline" onClick={() => {
                    updatePreference(preference);
                    onGetStarted();
                    navigate("/");
                }} className="mr-1">
                    Start
                </Button>
                <Button variant="outline">
                    <a href="https://github.com/braggs03/vscraper" target="_blank" className="">
                        Guide
                    </a>
                </Button>
            </div>
            <div className="flex items-center gap-3 mt-3">
                <Checkbox id="homepage_preference" checked={preference} onClick={() => setPreference(!preference)} />
                <Label htmlFor="homepage_preference">Don't Show on Start</Label>
            </div>
        </main>
    );
}
