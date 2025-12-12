import { useEffect, useState } from 'react';
import { Navigate, useNavigate } from 'react-router';
import "./App.css";
import { Button } from "./components/ui/button";
import { Checkbox } from "./components/ui/checkbox";
import { Input } from "./components/ui/input";
import { Label } from "./components/ui/label";
import { Progress } from "./components/ui/progress";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "./components/ui/select";
import { DownloadProgress } from './types';

const getPreference = () => {
    
}

const DownloadPage = ({ hasSeenHomepage }: { hasSeenHomepage: boolean }) => {
    const [config, setConfig] = useState(null);
    useEffect(() => {
        getPreference().then(setConfig);
    }, []);

    if (!config.skip_homepage && !hasSeenHomepage) {
        return <Navigate to="/starter" />;
    }

    const [url, setUrl] = useState('');
    const navigate = useNavigate();
    const [quality, setQuality] = useState('Best');
    const [format, setFormat] = useState('MP4');
    const [isAdvancedOptionsOpen, setIsAdvancedOptionsOpen] = useState(false);
    const [downloads, setDownloads] = useState<{ [key: string]: DownloadProgress }>({});
    const [isDownloading, setIsDownloading] = useState(false);
    const [downloadError, setDownloadError] = useState<string | null>(null);
    const [advancedOptions, setAdvancedOptions] = useState({
        autoStart: 'Yes',
        downloadFolder: 'Default',
        customNamePrefix: 'Default',
        itemsLimit: 'Default',
        strictPlaylistMode: false
    });


    const handleDownload = async () => {
        if (!url) return;

        setIsDownloading(true);
        setDownloadError(null);

        try {
            const options = {
                url,
                quality,
                format,
                ...advancedOptions
            };
        } catch (error) {
            console.error('Download failed:', error);
            setIsDownloading(false);
            setDownloadError('Failed to start download');
        }
    };

    const cancelDownload = async (downloadUrl: string) => {
        try {
            const { [downloadUrl]: _, ...remainingDownloads } = downloads;
            setDownloads(remainingDownloads);
        } catch (error) {
            console.error('Cancel download failed:', error);
        }
    };

    return (
        <main className="flex flex-col items-center justify-center text-center min-h-screen max-w-md mx-auto space-y-4">
            <div className="flex space-x-2">
                <Input
                    placeholder="Enter video or playlist URL"
                    value={url}
                    onChange={(e) => setUrl(e.target.value)}
                    className="grow"
                    disabled={isDownloading}
                />
                <Button
                    onClick={handleDownload}
                    disabled={isDownloading || !url}
                >
                    Download
                </Button>
            </div>

            {downloadError && (
                <div className="text-red-500 text-sm mb-2">
                    {downloadError}
                </div>
            )}

            <div className="flex space-x-2">
                <Select value={quality} onValueChange={setQuality} disabled={isDownloading}>
                    <SelectTrigger className="w-full">
                        <SelectValue placeholder="Quality" />
                    </SelectTrigger>
                    <SelectContent>
                        <SelectItem value="Best">Best</SelectItem>
                        <SelectItem value="1080p">1080p</SelectItem>
                        <SelectItem value="720p">720p</SelectItem>
                        <SelectItem value="480p">480p</SelectItem>
                    </SelectContent>
                </Select>

                <Select value={format} onValueChange={setFormat} disabled={isDownloading}>
                    <SelectTrigger className="w-full">
                        <SelectValue placeholder="Format" />
                    </SelectTrigger>
                    <SelectContent>
                        <SelectItem value="MP4">MP4</SelectItem>
                        <SelectItem value="MKV">MKV</SelectItem>
                        <SelectItem value="AVI">AVI</SelectItem>
                        <SelectItem value="WebM">WebM</SelectItem>
                    </SelectContent>
                </Select>
            </div>

            <Button
                variant="outline"
                className="w-full"
                onClick={() => setIsAdvancedOptionsOpen(!isAdvancedOptionsOpen)}
                disabled={isDownloading}
            >
                Advanced Options
            </Button>

            {isAdvancedOptionsOpen && (
                <div className="space-y-4 p-4 border rounded-md">
                    <div className="flex space-x-2">
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

                    <div className="flex space-x-2">
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

                    <div className="flex items-center space-x-2">
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

                    <div className="flex flex-col space-x-2 mt-4 space-y-2">
                        <Button variant="outline" className="w-full">Import URLs</Button>
                        <Button variant="outline" className="w-full">Export URLs</Button>
                        <Button variant="outline" className="w-full">Copy URLs</Button>
                    </div>
                </div>
            )}

            {(
                <div className="mt-4">
                    <h3 className="text-lg font-semibold mb-2">Downloading</h3>
                    <div className="space-y-2">
                        {Object.entries(downloads).map(([url, download]) => (
                            <div
                                key={url}
                                className="flex items-center space-x-2 p-2 border rounded-md"
                            >
                                <div className="grow">
                                    <div className="flex justify-between">
                                        <span className="text-sm truncate max-w-[200px]">{url}</span>
                                        <span className="text-sm">{download.percent}%</span>
                                    </div>
                                    <div className="w-full bg-gray-200 rounded-full h-2.5 dark:bg-gray-700 mt-1">
                                        <Progress
                                            className="bg-blue-600 h-2.5 rounded-full"
                                            value={Number(download.percent)}
                                        ></Progress>
                                    </div>
                                    <div className="flex justify-between text-xs text-gray-500 mt-1">
                                        <span>{download.speed}</span>
                                        <span>ETA: {download.eta}</span>
                                    </div>
                                </div>
                                <div className="flex space-x-2">
                                    <Button
                                        variant="ghost"
                                        size="icon"
                                        onClick={() => cancelDownload(url)}
                                    >
                                    </Button>
                                    <Button variant="ghost" size="icon">
                                    </Button>
                                </div>
                            </div>
                        ))}
                    </div>
                </div>
            )}

            <Button variant="outline" className="w-full mb-2" onClick={() => navigate("/starter")}>
                Back to Main Menu
            </Button>
        </main>
    );
};

export default DownloadPage;
