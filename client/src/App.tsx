
import { useQuery } from '@tanstack/react-query';
import { useEffect, useState } from 'react';
import { Navigate } from 'react-router';
import Header from './components/Header';
import { Button } from "./components/ui/button";
import { Checkbox } from "./components/ui/checkbox";
import { Input } from "./components/ui/input";
import { Label } from "./components/ui/label";
import { Progress } from './components/ui/progress';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "./components/ui/select";
import "./index.css";
import type { APIResponse } from './types/APIResponse';
import type { DownloadEntry } from './types/DownloadEntry';
import type { DownloadOptions } from './types/DownloadOptions';
import type { DownloadUpdate } from './types/DownloadUpdate';

const DownloadPage = ({ hasSeenHomepage }: { hasSeenHomepage: boolean }) => {

    // <----- State ----->

    const [url, setUrl] = useState('');
    const [quality, setQuality] = useState('best');
    const [nameFormat, _] = useState('%(title)s.%(ext)s');
    const [container, setContainer] = useState('mp4');
    const [isAdvancedOptionsOpen, setIsAdvancedOptionsOpen] = useState(false);
    const [downloads, setDownloads] = useState<DownloadEntry[]>([]);
    const [isDownloading, setIsDownloading] = useState(false);
    const [downloadError, setDownloadError] = useState<string | null>(null);
    const [advancedOptions, setAdvancedOptions] = useState({
        autoStart: 'Yes',
        downloadFolder: 'Default',
        customNamePrefix: 'Default',
        itemsLimit: 'Default',
        strictPlaylistMode: false
    });

    useEffect(() => {
        const connect = () => {
            let ws = new WebSocket("/api/download/ws");

            ws.onclose = () => {
                console.log("download updated ws closed, retrying in 3s...");
                setTimeout(connect, 3000);
            };

            ws.onerror = () => {
                ws.close();
            };

            ws.onopen = () => {
                console.log("opened download ws");
            }

            ws.onmessage = (event) => {
                let response: APIResponse = JSON.parse(event.data);
                if (response.type) {
                    switch (response.type) {
                        case "Update":
                            const downloadUpdate: DownloadUpdate = JSON.parse(response.data);
                            console.log(`recieved update: ${JSON.stringify(downloadUpdate)}`);

                            setDownloads(oldDownloads =>
                                oldDownloads.map(download =>
                                    download.url === downloadUpdate.url
                                        ? { ...download, download: { ...download.download, progress: downloadUpdate.progress } }
                                        : download
                                )
                            );

                            break;

                        case "DownloadsChange":
                            let changedDownloads: DownloadEntry[] = JSON.parse(response.data);
                            console.log(`recieved downloads change: ${JSON.stringify(changedDownloads)}`);

                            setDownloads(changedDownloads);

                            break;

                        default:
                            const _: never = response.type;
                            return _;
                    }
                }
            };

            return () => {
                ws.close();
            };
        }

        connect();
    }, []);

    const { isPending: configIsPending, data: config } = useQuery({
        queryKey: ['config'],
        queryFn: () =>
            fetch("/api/config").then((res) =>
                res.json(),
            ),
    });

    // <----- Loading ----->

    if (configIsPending) return 'Loading...'

    // <----- App ----->

    if (!config.skip_homepage && !hasSeenHomepage) {
        return <Navigate to="/starter" />;
    }

    const handleDownload = async () => {
        setIsDownloading(true);
        setDownloadError(null);
        try {
            const options: DownloadOptions = {
                container: container,
                name_format: nameFormat,
                quality: quality
            }

            await fetch("/api/download", {
                method: "POST",
                body: JSON.stringify({
                    "url": url,
                    options,
                }),
                headers: {
                    "Content-Type": "application/json",
                }
            });
        } catch (error) {
            setDownloadError(`Failed to start download with error: ${error}`);
        } finally {
            setIsDownloading(false);
        }
    };

    const handleUrlDownloads = async () => {
        // let urls = await fetch(new URL("download/urls", api)).then((res) =>
        //     res.json(),
        // );

        // urls = urls.join('\n');

        // const file = new File([urls], "urls.txt", { type: "text/plain;charset=utf-8" });

        // saveAs(file);
    }

    return (
        <>
            <Header />
            <main className="flex flex-col items-center text-center space-y-4 mt-10 mr-4 mb-4 ml-4">
                <div className="flex flex-row w-full max-w-4xl">
                    <Input
                        placeholder="Enter video or playlist URL"
                        value={url}
                        className="rounded-r-none border-r-0"
                        onChange={(e) => setUrl(e.target.value)}
                        disabled={isDownloading}
                    />
                    <Button
                        onClick={handleDownload}
                        className="rounded-l-none border border-color"
                        disabled={isDownloading || !url || !/^(http(s)?:\/\/)?([\w-]+\.)+[\w-]+(\/[\w-./?%&=]*)?$/.test(url)}
                    >
                        Download
                    </Button>
                </div>

                {downloadError && (
                    <div className="text-red-500 text-sm mb-2">
                        {downloadError}
                    </div>
                )}

                <div className="flex flex-row w-full space-x-4 max-w-4xl">
                    <Select value={quality} onValueChange={setQuality} disabled={isDownloading}>
                        <SelectTrigger className="flex-1 w-full">
                            <SelectValue placeholder="Quality" />
                        </SelectTrigger>
                        <SelectContent>
                            <SelectItem value="best">Best</SelectItem>
                            <SelectItem value="1080">1080p</SelectItem>
                            <SelectItem value="720">720p</SelectItem>
                            <SelectItem value="480">480p</SelectItem>
                        </SelectContent>
                    </Select>

                    <Select value={container} onValueChange={setContainer} disabled={isDownloading}>
                        <SelectTrigger className="flex-1 w-full">
                            <SelectValue placeholder="Format" />
                        </SelectTrigger>
                        <SelectContent>
                            <SelectItem value="mp4">MP4</SelectItem>
                            <SelectItem value="mkv">MKV</SelectItem>
                            <SelectItem value="avi">AVI</SelectItem>
                            <SelectItem value="webm">WebM</SelectItem>
                        </SelectContent>
                    </Select>

                    <Button
                        variant="outline"
                        className="flex-1 w-full"
                        onClick={() => setIsAdvancedOptionsOpen(!isAdvancedOptionsOpen)}
                        disabled={isDownloading}
                    >
                        Advanced Options
                    </Button>

                    {isAdvancedOptionsOpen && (
                        <div className="space-y-4 p-4 border rounded-md w-full max-w-md">
                            <div className="flex flex-wrap gap-4">
                                <div className="flex-1">
                                    <Label>Auto Start</Label>
                                    <Select
                                        value={advancedOptions.autoStart}
                                        onValueChange={(value) => setAdvancedOptions(prev => ({
                                            ...prev,
                                            autoStart: value
                                        }))}
                                    >
                                        <SelectTrigger>
                                            <SelectValue placeholder="Auto Start" />
                                        </SelectTrigger>
                                        <SelectContent>
                                            <SelectItem value="Yes">Yes</SelectItem>
                                            <SelectItem value="No">No</SelectItem>
                                        </SelectContent>
                                    </Select>
                                </div>
                                <div className="flex-1">
                                    <Label>Download Folder</Label>
                                    <Select
                                        value={advancedOptions.downloadFolder}
                                        onValueChange={(value) => setAdvancedOptions(prev => ({
                                            ...prev,
                                            downloadFolder: value
                                        }))}
                                    >
                                        <SelectTrigger>
                                            <SelectValue placeholder="Download Folder" />
                                        </SelectTrigger>
                                        <SelectContent>
                                            <SelectItem value="Default">Default</SelectItem>
                                            <SelectItem value="Custom">Custom</SelectItem>
                                        </SelectContent>
                                    </Select>
                                </div>
                            </div>

                            <div className="flex flex-wrap gap-4">
                                <div className="flex-1">
                                    <Label>Custom Name Prefix</Label>
                                    <Input
                                        placeholder="Default"
                                        value={advancedOptions.customNamePrefix}
                                        onChange={(e) => setAdvancedOptions(prev => ({
                                            ...prev,
                                            customNamePrefix: e.target.value
                                        }))}
                                    />
                                </div>
                                <div className="flex-1">
                                    <Label>Items Limit</Label>
                                    <Select
                                        value={advancedOptions.itemsLimit}
                                        onValueChange={(value) => setAdvancedOptions(prev => ({
                                            ...prev,
                                            itemsLimit: value
                                        }))}
                                    >
                                        <SelectTrigger>
                                            <SelectValue placeholder="Items Limit" />
                                        </SelectTrigger>
                                        <SelectContent>
                                            <SelectItem value="Default">Default</SelectItem>
                                            <SelectItem value="5">5</SelectItem>
                                            <SelectItem value="10">10</SelectItem>
                                            <SelectItem value="25">25</SelectItem>
                                        </SelectContent>
                                    </Select>
                                </div>
                            </div>

                            <div className="flex items-center space-x-2 mt-4">
                                <Checkbox
                                    id="strict-playlist-mode"
                                    checked={advancedOptions.strictPlaylistMode}
                                    onCheckedChange={(checked) => setAdvancedOptions(prev => ({
                                        ...prev,
                                        strictPlaylistMode: !!checked
                                    }))}
                                />
                                <Label htmlFor="strict-playlist-mode">Strict Playlist Mode</Label>
                            </div>

                            <div className="flex flex-wrap justify-center gap-2 mt-4">
                                <Button variant="outline" className="flex-1">Import URLs</Button>
                                <Button variant="outline" className="flex-1" onClick={() => handleUrlDownloads()}>Export URLs</Button>
                            </div>
                        </div>
                    )}
                </div>

                <h2 className="flex w-full border-t border-b text-4xl justify-center py-3">
                    <p className="max-w-5xl w-full text-left">
                        Downloading
                    </p>
                </h2>
                <div className="mt-4">
                    <div className="space-y-2">
                        {downloads.map(entry => (
                            <div
                                key={entry.url}
                                className="flex items-center space-x-2 p-2 border rounded-md"
                            >
                                <div className="grow">
                                    <div className="flex justify-between">
                                        <span className="text-sm truncate max-w-50">{url}</span>
                                        <span className="text-sm">{entry.download.progress.percent}%</span>
                                    </div>
                                    <div className="w-full bg-gray-200 rounded-full h-2.5 dark:bg-gray-700 mt-1">
                                        <Progress
                                            className="bg-blue-600 h-2.5 rounded-full"
                                            value={Number(entry.download.progress.percent)}
                                        ></Progress>
                                    </div>
                                    <div className="flex justify-between text-xs text-gray-500 mt-1">
                                        <span>{entry.download.progress.speed}</span>
                                        <span>ETA: {entry.download.progress.eta}</span>
                                    </div>
                                </div>
                                <div className="flex space-x-2">
                                    <Button
                                        variant="ghost"
                                        size="icon"
                                    >
                                    </Button>
                                    <Button variant="ghost" size="icon">
                                    </Button>
                                </div>
                            </div>
                        ))}
                    </div>
                </div>
            </main>
        </>
    );
};

export default DownloadPage;